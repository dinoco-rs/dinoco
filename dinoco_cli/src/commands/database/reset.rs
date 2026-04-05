use std::env;
use std::fs::read_to_string;
use std::path::Path;
use std::time::Duration;

use colored::*;
use indicatif::ProgressBar;
use inquire::Confirm;

use dinoco_compiler::{ConnectionUrl, Database, ParsedConfig};
use dinoco_compiler::{compile, render_error};
use dinoco_engine::{DinocoAdapter, DinocoAdapterHandler, DinocoResult, MySqlAdapter, PostgresAdapter, SqliteAdapter};

pub async fn reset_database() -> DinocoResult<()> {
    let schema_path = "dinoco/schema.dinoco";

    if !Path::new(schema_path).exists() {
        println!("\n{} {}\n", "✖".red().bold(), "Dinoco project not initialized.".bold());
        return Ok(());
    }

    println!("{} {}", "✔".green().bold(), "Starting database reset...".white());

    let pb = ProgressBar::new_spinner();
    pb.enable_steady_tick(Duration::from_millis(80));
    pb.set_message("Compiling schema...");

    let source = match read_to_string(schema_path) {
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
            println!("\n{} {}\n", "✖".red().bold(), format!("Schema compilation failed ({} error(s)).", errs.len()).bold());

            for err in errs {
                println!("{}", render_error(&err, &source));
            }

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
                    return Ok(());
                }
            },
            ConnectionUrl::Literal(url) => url.clone(),
        };

        (url, database.clone())
    };

    pb.set_message(format!("Connecting to {:?}...", db_type));

    match db_type {
        Database::Postgresql => match PostgresAdapter::connect(url).await {
            Ok(adapter) => {
                pb.suspend(|| println!("{} {}", "✔".green().bold(), "Connected to database.".white()));
                execute_reset(adapter, &pb).await?;
            }
            Err(err) => {
                pb.finish_and_clear();
                println!("\n{} {}\n", "✖".red().bold(), "Database connection failed.".bold());
                println!("  {} {}", "Reason:".yellow().bold(), err.to_string().white());
            }
        },
        Database::Mysql => match MySqlAdapter::connect(url).await {
            Ok(adapter) => {
                pb.suspend(|| println!("{} {}", "✔".green().bold(), "Connected to database.".white()));
                execute_reset(adapter, &pb).await?;
            }
            Err(err) => {
                pb.finish_and_clear();
                println!("\n{} {}\n", "✖".red().bold(), "Database connection failed.".bold());
                println!("  {} {}", "Reason:".yellow().bold(), err.to_string().white());
            }
        },
        Database::Sqlite => match SqliteAdapter::connect(url).await {
            Ok(adapter) => {
                pb.suspend(|| println!("{} {}", "✔".green().bold(), "Connected to database.".white()));
                execute_reset(adapter, &pb).await?;
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

async fn execute_reset<T>(adapter: T, pb: &ProgressBar) -> DinocoResult<()>
where
    T: DinocoAdapter + DinocoAdapterHandler,
{
    pb.set_message("Inspecting current database state...");

    let tables = adapter.fetch_tables().await?;

    pb.finish_and_clear();

    println!(
        "{} {}",
        "⚠".yellow().bold(),
        format!("This will delete all {} table(s) from the database.", tables.len()).yellow()
    );
    println!("{} {}", "ℹ".blue(), "Local migration files will be kept intact.".bright_black());

    match Confirm::new("Are you sure you want to reset the database?").with_default(false).prompt() {
        Ok(true) => {
            println!("{} {}", "⚠".yellow().bold(), "Resetting database...".yellow());
        }
        _ => {
            println!("{} {}", "✗".red().bold(), "Database reset cancelled.".white());

            return Ok(());
        }
    }

    let reset_pb = ProgressBar::new_spinner();
    reset_pb.enable_steady_tick(Duration::from_millis(80));
    reset_pb.set_message("Dropping database objects...");

    adapter.reset_database().await?;

    reset_pb.finish_and_clear();

    println!("{} {}", "✔".green().bold(), "Database reset completed successfully!".white());
    println!("{} {}", "✔".green().bold(), "Migration history cleared.".white());
    println!("{} {}", "ℹ".blue(), "Local migration files were kept intact.".bright_black());

    Ok(())
}
