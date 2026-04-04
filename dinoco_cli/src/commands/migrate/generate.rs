use std::env;
use std::fs::{self, read_to_string};
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use colored::*;
use indicatif::ProgressBar;
use inquire::{Confirm, Text};

use dinoco_codegen::generate_models;
use dinoco_compiler::{ConnectionUrl, Database, ParsedConfig, ParsedSchema};
use dinoco_compiler::{compile, render_error};
use dinoco_engine::{DinocoAdapter, DinocoAdapterHandler, DinocoResult, MigrationExecutor, MySqlAdapter, PostgresAdapter, SafetyLevel, SqliteAdapter, calculate_diff};

use crate::{create_migration_table, decode_schema, encode_schema, get_last_migration, insert_migration, normalize_schema, to_snake_case};

pub async fn generate_migrate() -> DinocoResult<()> {
    let schema_path = "dinoco/schema.dinoco";

    if !Path::new(schema_path).exists() {
        println!("\n{} {}\n", "✖".red().bold(), "Dinoco project not initialized.".bold());
        println!("  {} {}", "→ Missing schema file:".yellow().bold(), schema_path.cyan());
        println!("\n{} {}\n", "Hint:".blue().bold(), "Run the command below to initialize your project:".white());
        println!("  {} {}\n", "dinoco init".green().bold(), "(creates the required project structure)".dimmed());

        return Ok(());
    }

    println!("{} {}", "✔".green().bold(), "Schema found. Starting migration...".white());

    let pb = ProgressBar::new_spinner();
    pb.enable_steady_tick(Duration::from_millis(80));
    pb.set_message("Compiling schema...");

    let source = match read_to_string(schema_path) {
        Ok(content) => content,
        Err(e) => {
            pb.finish_and_clear();

            println!("\n{} {}\n", "✖".red().bold(), "Failed to read schema file.".bold());
            println!("  {} {}", "Reason:".yellow().bold(), e.to_string().white());

            return Ok(());
        }
    };

    let parsed = match compile(&source) {
        Ok((_, parsed)) => {
            pb.suspend(|| {
                println!("{} {}", "✔".green().bold(), "Schema compiled successfully.".white());
            });

            parsed
        }
        Err(errs) => {
            pb.finish_and_clear();
            println!("\n{} {}\n", "✖".red().bold(), format!("Schema compilation failed ({} error(s)).", errs.len()).bold());

            for err in errs {
                println!("{}", render_error(&err, &source));
            }

            println!("\n{} {}\n", "Hint:".blue().bold(), "Fix the errors above and run the command again.".white());

            return Ok(());
        }
    };

    let (url, db_type) = {
        let ParsedConfig { database, database_url, .. } = &parsed.config;

        let url = match database_url {
            ConnectionUrl::Env(var_name) => match env::var(var_name) {
                Ok(val) => val,
                Err(_) => {
                    pb.finish_and_clear();

                    println!("\n{} {}\n", "✖".red().bold(), "Missing environment variable.".bold());
                    println!("  {} {}", "→ Variable:".yellow().bold(), var_name.cyan());
                    println!("  {} {}", "Hint:".blue().bold(), format!("Define {} in your environment or .env file.", var_name).white());

                    return Ok(());
                }
            },
            ConnectionUrl::Literal(url) => url.clone(),
        };

        (url, database.clone())
    };

    pb.set_message(format!("Connecting to {:?} database...", db_type));

    match db_type {
        Database::Postgresql => match PostgresAdapter::connect(url).await {
            Ok(adapter) => {
                pb.suspend(|| println!("{} {}", "✔".green().bold(), "Connected to database.".white()));

                execute_migrate(adapter, &pb, parsed).await?;
            }
            Err(e) => {
                pb.finish_and_clear();

                println!("\n{} {}\n", "✖".red().bold(), "Database connection failed.".bold());
                println!("  {} {}", "Reason:".yellow().bold(), e.to_string().white());
            }
        },
        Database::Mysql => match MySqlAdapter::connect(url).await {
            Ok(adapter) => {
                pb.suspend(|| println!("{} {}", "✔".green().bold(), "Connected to database.".white()));

                execute_migrate(adapter, &pb, parsed).await?;
            }
            Err(e) => {
                pb.finish_and_clear();

                println!("\n{} {}\n", "✖".red().bold(), "Database connection failed.".bold());
                println!("  {} {}", "Reason:".yellow().bold(), e.to_string().white());
            }
        },
        Database::Sqlite => match SqliteAdapter::connect(url).await {
            Ok(adapter) => {
                pb.suspend(|| println!("{} {}", "✔".green().bold(), "Connected to database.".white()));
                execute_migrate(adapter, &pb, parsed).await?;
            }
            Err(e) => {
                pb.finish_and_clear();
                println!("\n{} {}\n", "✖".red().bold(), "Database connection failed.".bold());
                println!("  {} {}", "Reason:".yellow().bold(), e.to_string().white());
            }
        },
    }

    Ok(())
}

async fn execute_migrate<T>(adapter: T, pb: &ProgressBar, parsed_schema: ParsedSchema) -> DinocoResult<()>
where
    T: DinocoAdapter + DinocoAdapterHandler + MigrationExecutor,
{
    pb.set_message("Fetching current database state...");

    let tables = adapter.fetch_tables().await?;
    let has_migration_history = tables.iter().any(|table| table.name == "_dinoco_migrations");

    if !tables.is_empty() && !has_migration_history {
        let should_reset = pb.suspend(|| {
            let prompt_msg = "This database already contains data, but no migration history was found.\n  Do you want to reset the database and apply your new schema?";

            match Confirm::new(prompt_msg).with_default(false).prompt() {
                Ok(true) => {
                    println!("{} {}", "⚠".yellow().bold(), "Proceeding with database reset...".yellow());
                    true
                }
                Ok(false) => {
                    println!("{} {}", "✗".red().bold(), "Migration cancelled by user.".white());
                    false
                }
                Err(_) => {
                    println!("{} {}", "✗".red().bold(), "Prompt error. Migration cancelled.".white());
                    false
                }
            }
        });

        if !should_reset {
            pb.finish_and_clear();
            return Ok(());
        }

        pb.set_message("Resetting database...");
        adapter.reset_database().await?;
        create_migration_table(&adapter).await?;
    } else if !has_migration_history {
        pb.set_message("Initializing migration table...");
        create_migration_table(&adapter).await?;
    }

    pb.set_message("Fetching last migration...");

    let last_schema = get_last_migration(&adapter).await?.map(|migration| decode_schema(&migration.schema));

    pb.set_message("Calculating schema diff...");

    let mut normalized_schema = parsed_schema.clone();
    normalize_schema(&mut normalized_schema);

    let plan = calculate_diff(&last_schema, &normalized_schema);

    pb.finish_and_clear();

    if plan.steps.is_empty() {
        println!("{} {}", "✔".green().bold(), "No changes detected.".white());
        println!("  {} Your schema is already up to date.", "└─".dimmed());
        return Ok(());
    }

    println!("{} {}", "✔".green().bold(), format!("Detected {} pending change(s).", plan.steps.len()).white());

    if !plan.safety_alerts.is_empty() {
        let is_destructive = plan.is_destructive();

        println!(
            "\n{} {}",
            "⚠".yellow().bold(),
            if is_destructive {
                "WARNING: This migration may cause data loss!".red().bold()
            } else {
                "This migration needs extra attention.".yellow().bold()
            }
        );

        for alert in plan.safety_alerts.iter().take(5) {
            match alert {
                SafetyLevel::Destructive(message) => {
                    println!("  {} {}", "•".red(), message.yellow());
                }
                SafetyLevel::Warning(message) => {
                    println!("  {} {}", "•".yellow(), message.white());
                }
            }
        }

        if plan.safety_alerts.len() > 5 {
            println!("  {} ...and {} more item(s).", "•".yellow(), plan.safety_alerts.len() - 5);
        }

        let confirm = if is_destructive {
            "Are you sure you want to proceed and permanently apply these destructive changes?"
        } else {
            "Do you want to proceed with this migration?"
        };

        match Confirm::new(confirm).with_default(false).prompt() {
            Ok(true) => {
                println!("{} {}", "⚠".yellow().bold(), "Proceeding with migration...".yellow());
            }
            _ => {
                println!("{} {}", "✗".red().bold(), "Migration cancelled for safety.".white());
                return Ok(());
            }
        }
    }

    let migration_name = loop {
        match Text::new("Enter a name for the new migration (e.g., AddedTesting):").prompt() {
            Ok(input_name) => {
                let trimmed = input_name.trim();

                if trimmed.is_empty() {
                    println!("{} {}", "⚠".yellow().bold(), "Migration name cannot be empty. Please try again.".white());

                    continue;
                }

                let snake_name = to_snake_case(trimmed);
                let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
                let proposed_name = format!("{}_{}", timestamp, snake_name);
                let migration_dir = format!("dinoco/migrations/{}", proposed_name);

                if Path::new(&migration_dir).exists() {
                    println!(
                        "{} {}",
                        "✖".red().bold(),
                        "A migration with this name/timestamp already exists. Please wait a second or use a different name.".white()
                    );
                } else {
                    break proposed_name;
                }
            }
            Err(_) => {
                println!("{} {}", "✗".red().bold(), "Prompt error. Migration cancelled.".white());
                return Ok(());
            }
        }
    };

    let sqls = adapter.build_migration(&plan.steps, &normalized_schema, false);

    if sqls.is_empty() {
        println!("{} {}", "✔".green().bold(), "No executable SQL was generated for this migration.".white());
        return Ok(());
    }

    pb.set_message("Applying migration to database...");

    for sql in &sqls {
        if let Err(err) = adapter.execute(sql, &[]).await {
            pb.finish_and_clear();
            println!("{} {}", "✖".red().bold(), "Failed to execute migration SQL.".white());
            println!("  {} {}", "Statement:".yellow().bold(), sql.cyan());
            println!("  {} {}", "Reason:".yellow().bold(), err.to_string().white());
            return Ok(());
        }
    }

    pb.set_message("Saving migration history...");

    let schema_bytes = encode_schema(normalized_schema);
    insert_migration(&adapter, &migration_name, schema_bytes.clone()).await?;

    let migration_dir = format!("dinoco/migrations/{migration_name}");

    fs::create_dir_all(&migration_dir).unwrap();

    fs::write(format!("{migration_dir}/migration.sql"), sqls.join("\n\n")).unwrap();
    // fs::write(format!("{migration_dir}/schema.bin"), schema_bytes).unwrap();

    pb.finish_and_clear();

    println!("{} {}", "✔".green().bold(), "Migration applied successfully!".white());
    println!("{} {}", "→".cyan().bold(), "Generating models...".dimmed());

    generate_models(parsed_schema);

    println!("{} {}", "✔".green().bold(), "Models generated successfully!".white());

    Ok(())
}
