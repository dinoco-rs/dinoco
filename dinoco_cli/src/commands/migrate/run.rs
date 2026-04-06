use std::env;
use std::fs;
use std::path::Path;
use std::time::Duration;

use colored::*;
use indicatif::ProgressBar;

use dinoco_codegen::generate_models;
use dinoco_compiler::{ConnectionUrl, Database, ParsedConfig, ParsedSchema, compile, render_error};
use dinoco_engine::{
    DinocoAdapter, DinocoAdapterHandler, DinocoClientConfig, DinocoResult, MySqlAdapter, PostgresAdapter, SqliteAdapter,
};

use crate::{
    create_migration_table, execute_migration_file, get_all_migrations, local_migration_names, mark_migration_applied,
};

pub async fn run_migrations() -> DinocoResult<()> {
    let schema_path = "dinoco/schema.dinoco";

    if !Path::new(schema_path).exists() {
        println!("\n{} {}\n", "✖".red().bold(), "Dinoco project not initialized.".bold());

        return Ok(());
    }

    println!("{} {}", "✔".green().bold(), "Starting migration execution...".white());

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

            return Ok(());
        }
    };

    let (url, database) = match resolve_database_url(&parsed.config, &pb) {
        Some(result) => result,
        None => return Ok(()),
    };

    pb.set_message(format!("Connecting to {:?}...", database));

    match database {
        Database::Postgresql => match PostgresAdapter::connect(url, DinocoClientConfig::default()).await {
            Ok(adapter) => {
                pb.suspend(|| println!("{} {}", "✔".green().bold(), "Connected to database.".white()));
                execute_run(adapter, &pb, parsed).await?;
            }
            Err(err) => {
                pb.finish_and_clear();
                println!("\n{} {}\n", "✖".red().bold(), "Database connection failed.".bold());
                println!("  {} {}", "Reason:".yellow().bold(), err.to_string().white());
            }
        },
        Database::Mysql => match MySqlAdapter::connect(url, DinocoClientConfig::default()).await {
            Ok(adapter) => {
                pb.suspend(|| println!("{} {}", "✔".green().bold(), "Connected to database.".white()));
                execute_run(adapter, &pb, parsed).await?;
            }
            Err(err) => {
                pb.finish_and_clear();
                println!("\n{} {}\n", "✖".red().bold(), "Database connection failed.".bold());
                println!("  {} {}", "Reason:".yellow().bold(), err.to_string().white());
            }
        },
        Database::Sqlite => match SqliteAdapter::connect(url, DinocoClientConfig::default()).await {
            Ok(adapter) => {
                pb.suspend(|| println!("{} {}", "✔".green().bold(), "Connected to database.".white()));
                execute_run(adapter, &pb, parsed).await?;
            }
            Err(err) => {
                pb.finish_and_clear();
                println!("\n{} {}\n", "✖".red().bold(), "Database connection failed.".bold());
                println!("  {} {}", "Reason:".yellow().bold(), err.to_string().white());
            }
        },
    }

    Ok(())
}

async fn execute_run<T>(adapter: T, pb: &ProgressBar, parsed_schema: ParsedSchema) -> DinocoResult<()>
where
    T: DinocoAdapter + DinocoAdapterHandler + Sync,
{
    pb.set_message("Checking migration history...");

    let tables = adapter.fetch_tables().await?;
    let has_history_table = tables.iter().any(|table| table.name == "_dinoco_migrations");

    let local_names = local_migration_names()?;
    let pending = if has_history_table {
        let migrations = get_all_migrations(&adapter).await?;

        local_names
            .into_iter()
            .filter(|migration_name| {
                migrations
                    .iter()
                    .find(|migration| migration.name == *migration_name)
                    .is_none_or(|migration| migration.applied_at.is_none() || migration.rollback_at.is_some())
            })
            .collect::<Vec<_>>()
    } else {
        pb.set_message("Initializing migration history...");
        create_migration_table(&adapter).await?;

        local_names
    };

    pb.finish_and_clear();

    if pending.is_empty() {
        println!("{} {}", "✔".green().bold(), "The database is already up to date.".white());

        return Ok(());
    }

    println!("{} {}", "✔".green().bold(), format!("Found {} pending migration(s).", pending.len()).white());

    for migration_name in pending {
        println!("  {} Applying '{}'...", "→".cyan().bold(), migration_name);

        if let Err(err) = execute_migration_file(&adapter, &migration_name).await {
            println!("    {} {} {}", "✖".red(), "Failed to execute migration:".bold(), migration_name.yellow());
            println!("    {} {}", "Reason:".red(), err);

            return Ok(());
        }

        mark_migration_applied(&adapter, &migration_name).await?;
        println!("    {} Applied successfully.", "✔".green());
    }

    generate_models(parsed_schema);

    println!("\n{} {}", "✔".green().bold(), "All pending migrations were applied successfully.".white());

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

                return None;
            }
        },
        ConnectionUrl::Literal(url) => url.clone(),
    };

    Some((url, database.clone()))
}
