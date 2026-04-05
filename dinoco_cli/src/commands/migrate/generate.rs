use std::env;
use std::fs;
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use colored::*;
use indicatif::ProgressBar;
use inquire::{Confirm, Text};

use dinoco_codegen::generate_models;
use dinoco_compiler::{ConnectionUrl, Database, ParsedConfig, ParsedSchema, compile, render_error};
use dinoco_engine::{
    DinocoAdapter, DinocoAdapterHandler, DinocoError, DinocoResult, MigrationExecutor, MySqlAdapter, PostgresAdapter,
    SafetyLevel, SqliteAdapter, calculate_diff,
};

use crate::{
    create_migration_table, insert_migration, mark_migration_applied, read_latest_local_schema, to_snake_case,
    write_migration_schema,
};

pub async fn generate_migrate(apply: bool) -> DinocoResult<()> {
    let schema_path = "dinoco/schema.dinoco";

    if !Path::new(schema_path).exists() {
        println!("\n{} {}\n", "✖".red().bold(), "Dinoco project not initialized.".bold());
        println!("  {} {}", "→ Missing schema file:".yellow().bold(), schema_path.cyan());
        println!("\n{} {}\n", "Hint:".blue().bold(), "Run the command below to initialize the project:".white());
        println!("  {} {}\n", "dinoco init".green().bold(), "(creates the required project structure)".dimmed());

        return Ok(());
    }

    println!("{} {}", "✔".green().bold(), "Schema found. Starting migration generation...".white());

    let pb = ProgressBar::new_spinner();
    pb.enable_steady_tick(Duration::from_millis(80));
    pb.set_message("Compiling schema...");

    let source = match fs::read_to_string(schema_path) {
        Ok(content) => content,
        Err(err) => {
            pb.finish_and_clear();

            println!("\n{} {}\n", "✖".red().bold(), "Failed to read schema file.".bold());
            println!("  {} {}", "Reason:".yellow().bold(), err.to_string().white());

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

            println!(
                "\n{} {}\n",
                "✖".red().bold(),
                format!("Schema compilation failed ({} error(s)).", errs.len()).bold()
            );

            for err in errs {
                println!("{}", render_error(&err, &source));
            }

            println!("\n{} {}\n", "Hint:".blue().bold(), "Fix the errors above and run the command again.".white());

            return Ok(());
        }
    };

    let (url, database) = match resolve_database_url(&parsed.config, &pb) {
        Some(result) => result,
        None => return Ok(()),
    };

    pb.set_message("Reading latest migration schema...");

    let current_state = read_latest_local_schema()?;

    pb.set_message(format!("Connecting to {:?}...", database));

    match database {
        Database::Postgresql => execute_migrate::<PostgresAdapter>(&pb, parsed, current_state, url, apply).await?,
        Database::Mysql => execute_migrate::<MySqlAdapter>(&pb, parsed, current_state, url, apply).await?,
        Database::Sqlite => execute_migrate::<SqliteAdapter>(&pb, parsed, current_state, url, apply).await?,
    }

    Ok(())
}

async fn execute_migrate<T>(
    pb: &ProgressBar,
    parsed_schema: ParsedSchema,
    current_state: Option<ParsedSchema>,
    database_url: String,
    apply: bool,
) -> DinocoResult<()>
where
    T: DinocoAdapter + DinocoAdapterHandler + MigrationExecutor,
{
    let adapter = T::connect(database_url.clone()).await?;

    pb.set_message("Calculating schema diff...");

    let plan = calculate_diff(&current_state, &parsed_schema);

    pb.finish_and_clear();

    if plan.steps.is_empty() {
        println!("{} {}", "✔".green().bold(), "No changes detected.".white());
        println!("  {} Your schema is already up to date.", "└─".dimmed());

        return Ok(());
    }

    println!("{} {}", "✔".green().bold(), format!("Detected {} pending change(s).", plan.steps.len()).white());

    if !confirm_plan(&plan.safety_alerts)? {
        println!("{} {}", "✗".red().bold(), "Migration generation was cancelled for safety.".white());

        return Ok(());
    }

    let migration_name = prompt_migration_name()?;
    let sqls = adapter.build_migration(&plan.steps, &parsed_schema, false);

    if sqls.is_empty() {
        println!("{} {}", "✔".green().bold(), "No executable SQL was generated for this migration.".white());

        return Ok(());
    }

    let migration_dir = format!("dinoco/migrations/{migration_name}");

    fs::create_dir_all(&migration_dir)?;
    fs::write(format!("{migration_dir}/migration.sql"), sqls.join("\n\n"))?;
    write_migration_schema(&migration_name, &parsed_schema)?;
    create_migration_table(&adapter).await?;
    insert_migration(&adapter, &migration_name).await?;

    println!("{} {}", "✔".green().bold(), "Migration files generated successfully.".white());

    if apply {
        println!("{} {}", "→".cyan().bold(), "Applying the migration to the primary database...".dimmed());

        apply_generated_migration(&adapter, &migration_name, &sqls).await?;

        println!("{} {}", "→".cyan().bold(), "Generating Rust models...".dimmed());

        generate_models(parsed_schema);

        println!("{} {}", "✔".green().bold(), "Rust models generated successfully.".white());
    } else {
        println!(
            "{} {}",
            "ℹ".blue(),
            "Migration generated only. Use `--apply` to apply it now, or `dinoco migrate run` later.".bright_black()
        );
    }

    Ok(())
}

fn resolve_database_url(config: &ParsedConfig, pb: &ProgressBar) -> Option<(String, Database)> {
    let ParsedConfig { database, database_url, .. } = config;

    let url = match database_url {
        ConnectionUrl::Env(var_name) => match env::var(var_name) {
            Ok(value) => value,
            Err(_) => {
                pb.finish_and_clear();

                println!("\n{} {}\n", "✖".red().bold(), "Missing environment variable.".bold());
                println!("  {} {}", "→ Variable:".yellow().bold(), var_name.cyan());
                println!(
                    "  {} {}",
                    "Hint:".blue().bold(),
                    format!("Define {} in your environment or .env file.", var_name).white()
                );

                return None;
            }
        },
        ConnectionUrl::Literal(url) => url.clone(),
    };

    Some((url, database.clone()))
}

fn confirm_plan(safety_alerts: &[SafetyLevel]) -> DinocoResult<bool> {
    if safety_alerts.is_empty() {
        return Ok(true);
    }

    let is_destructive = safety_alerts.iter().any(|alert| matches!(alert, SafetyLevel::Destructive(_)));

    println!(
        "\n{} {}",
        "⚠".yellow().bold(),
        if is_destructive {
            "WARNING: This migration may cause data loss.".red().bold()
        } else {
            "This migration needs extra attention.".yellow().bold()
        }
    );

    for alert in safety_alerts.iter().take(5) {
        match alert {
            SafetyLevel::Destructive(message) => {
                println!("  {} {}", "•".red(), message.yellow());
            }
            SafetyLevel::Warning(message) => {
                println!("  {} {}", "•".yellow(), message.white());
            }
        }
    }

    if safety_alerts.len() > 5 {
        println!("  {} ...and {} more item(s).", "•".yellow(), safety_alerts.len() - 5);
    }

    let confirm_message = if is_destructive {
        "Are you sure you want to continue with this destructive migration?"
    } else {
        "Do you want to continue with this migration?"
    };

    Confirm::new(confirm_message).with_default(false).prompt().map_err(|err| DinocoError::ParseError(err.to_string()))
}

fn prompt_migration_name() -> DinocoResult<String> {
    loop {
        let input = Text::new("Enter a name for the new migration (for example: AddedTesting):")
            .prompt()
            .map_err(|err| DinocoError::ParseError(err.to_string()))?;

        let trimmed = input.trim();

        if trimmed.is_empty() {
            println!("{} {}", "⚠".yellow().bold(), "Migration name cannot be empty. Please try again.".white());

            continue;
        }

        let snake_name = to_snake_case(trimmed);
        let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
        let proposed_name = format!("{}_{}", timestamp, snake_name);
        let migration_dir = format!("dinoco/migrations/{proposed_name}");

        if Path::new(&migration_dir).exists() {
            println!(
                "{} {}",
                "✖".red().bold(),
                "A migration with this name and timestamp already exists. Please wait a second or use a different name."
                    .white()
            );

            continue;
        }

        return Ok(proposed_name);
    }
}

async fn apply_generated_migration<T>(adapter: &T, migration_name: &str, sqls: &[String]) -> DinocoResult<()>
where
    T: DinocoAdapter + DinocoAdapterHandler,
{
    for sql in sqls {
        adapter.execute(sql, &[]).await?;
    }

    mark_migration_applied(adapter, migration_name).await?;

    Ok(())
}
