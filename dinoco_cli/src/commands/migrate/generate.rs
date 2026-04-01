use std::env;
use std::fs::read_to_string;
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use colored::*;
use indicatif::ProgressBar;

use dinoco_compiler::{ConnectionUrl, Database, ParsedConfig, ParsedSchema, compile, render_error};
use dinoco_engine::{DinocoAdapter, DinocoResult, Migration, MigrationStep, MySqlAdapter, PostgresAdapter};
use inquire::Confirm;

use crate::commands::{encode_schema, insert_migration};
use crate::{DataCheck, create_migration_table, decode_schema, drop_all_tables, fetch, get_last_migration};

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
                execute_migrate(adapter, &pb, parsed, &Database::Postgresql).await?;
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
                execute_migrate(adapter, &pb, parsed, &Database::Mysql).await?;
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

pub async fn execute_migrate<T: DinocoAdapter>(adapter: T, pb: &ProgressBar, parsed_schema: ParsedSchema, db_type: &Database) -> DinocoResult<()> {
    pb.set_message("Fetching current database state...");

    let tables = fetch(&adapter, db_type).await?;
    let has_dinoco_migrations = tables.iter().any(|x| x.name == "_dinoco_migrations");

    if !tables.is_empty() && !has_dinoco_migrations {
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

        drop_all_tables(&adapter, tables).await?;
        create_migration_table(&adapter).await?;
    } else if !has_dinoco_migrations {
        pb.set_message("Initializing migration table...");
        create_migration_table(&adapter).await?;
    }

    pb.set_message("Fetching last migration...");

    let last_migration: Option<ParsedSchema> = if let Some(last) = get_last_migration(&adapter).await? {
        Some(decode_schema(&last.schema))
    } else {
        None
    };

    pb.set_message("Calculating schema diff...");

    let migration = Migration::new(&adapter, last_migration, parsed_schema.clone());
    let changes = migration.diff();

    pb.finish_and_clear();

    if changes.is_empty() {
        println!("{} {}", "✔".green().bold(), "No changes detected.".white());
        println!("  {} Your schema is already up to date.", "└─".dimmed());

        return Ok(());
    }

    println!("{} {}", "✔".green().bold(), format!("Detected {} pending change(s).", changes.len()).white());

    let mut has_data_loss = false;
    let mut loss_descriptions = Vec::new();

    let q = match db_type {
        Database::Postgresql => "\"",
        Database::Mysql => "`",
    };

    for change in &changes {
        match change {
            MigrationStep::DropTable(table_name) => {
                let query = format!("SELECT 1 as has_data FROM {q}{table_name}{q} LIMIT 1");
                if let Ok(res) = adapter.query_as::<DataCheck>(&query, &[]).await {
                    if !res.is_empty() {
                        has_data_loss = true;
                        loss_descriptions.push(format!("Table '{}' is going to be dropped.", table_name));
                    }
                }
            }
            MigrationStep::DropColumn { table_name, field } => {
                let query = format!(
                    "SELECT 1 as has_data FROM {q}{table_name}{q} WHERE {q}{col_name}{q} IS NOT NULL LIMIT 1",
                    table_name = table_name,
                    col_name = field.name
                );

                if let Ok(res) = adapter.query_as::<DataCheck>(&query, &[]).await {
                    if !res.is_empty() {
                        has_data_loss = true;
                        loss_descriptions.push(format!("Column '{}.{}' is going to be dropped.", table_name, field.name));
                    }
                }
            }
            _ => {}
        }
    }

    if has_data_loss {
        println!("\n{} {}", "⚠".yellow().bold(), "WARNING: This migration will cause DATA LOSS!".red().bold());

        for desc in loss_descriptions.iter().take(3) {
            println!("  {} {}", "•".red(), desc.yellow());
        }
        if loss_descriptions.len() > 3 {
            println!("  {} ...and {} more items.", "•".red(), loss_descriptions.len() - 3);
        }

        let confirm = Confirm::new("Are you sure you want to proceed and permanently DELETE this data?")
            .with_default(false)
            .prompt();

        match confirm {
            Ok(true) => {
                println!("{} {}", "⚠".yellow().bold(), "Proceeding with data deletion...".yellow());
            }
            _ => {
                println!("{} {}", "✗".red().bold(), "Migration cancelled to prevent data loss.".white());

                return Ok(());
            }
        }
    }

    let pb_exec = ProgressBar::new_spinner();
    pb_exec.enable_steady_tick(Duration::from_millis(80));
    pb_exec.set_message("Applying migration to database...");

    let sqls = migration.to_up_sql(changes);

    for sql in &sqls {
        adapter.execute(&sql, &[]).await?;
    }

    pb_exec.set_message("Saving migration history...");

    let schema_bytes = encode_schema(parsed_schema);
    let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    let migration_name = format!("{}_migration", timestamp);

    insert_migration(&adapter, &migration_name, schema_bytes).await?;

    std::fs::create_dir_all(format!("dinoco/migrations/{migration_name}")).unwrap();
    std::fs::write(format!("dinoco/migrations/{migration_name}/migration.sql"), sqls.join("\n\n")).unwrap();

    pb_exec.finish_and_clear();
    println!("{} {}", "✔".green().bold(), "Migration applied successfully!".white());

    println!("{} {}", "→".cyan().bold(), "Generating models...".dimmed());
    println!("{} {}", "✔".green().bold(), "Models generated successfully!".white());

    Ok(())
}
