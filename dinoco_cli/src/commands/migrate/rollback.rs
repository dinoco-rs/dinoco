use std::env;
use std::fs::read_to_string;
use std::path::Path;
use std::time::Duration;

use colored::*;
use indicatif::ProgressBar;
use inquire::Confirm;

use dinoco_compiler::{ConnectionUrl, Database, ParsedConfig};
use dinoco_compiler::{compile, render_error};

use dinoco_engine::calculate_diff;
use dinoco_engine::{
    DinocoAdapter, DinocoAdapterHandler, DinocoResult, MigrationExecutor, MySqlAdapter,
    PostgresAdapter, SqliteAdapter,
};

use crate::{create_migration_table, decode_schema, delete_migration, get_last_two_migrations};

pub async fn rollback_migration() -> DinocoResult<()> {
    let schema_path = "dinoco/schema.dinoco";

    if !Path::new(schema_path).exists() {
        println!(
            "\n{} {}\n",
            "✖".red().bold(),
            "Dinoco project not initialized.".bold()
        );
        return Ok(());
    }

    println!("{} {}", "✔".green().bold(), "Starting rollback...".white());

    let pb = ProgressBar::new_spinner();
    pb.enable_steady_tick(Duration::from_millis(80));
    pb.set_message("Compiling schema...");

    let source = match read_to_string(schema_path) {
        Ok(content) => content,
        Err(e) => {
            pb.finish_and_clear();
            println!(
                "\n{} {}\n",
                "✖".red().bold(),
                "Failed to read schema file.".bold()
            );
            println!("  {} {}", "Reason:".yellow().bold(), e.to_string().white());
            return Ok(());
        }
    };

    let parsed = match compile(&source) {
        Ok((_, parsed)) => {
            pb.suspend(|| {
                println!(
                    "{} {}",
                    "✔".green().bold(),
                    "Schema compiled successfully.".white()
                );
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

    let (url, db_type) = {
        let ParsedConfig {
            database,
            database_url,
            ..
        } = &parsed.config;

        let url = match database_url {
            ConnectionUrl::Env(var_name) => match env::var(var_name) {
                Ok(val) => val,
                Err(_) => {
                    pb.finish_and_clear();
                    println!(
                        "\n{} {}\n",
                        "✖".red().bold(),
                        "Missing environment variable.".bold()
                    );
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
                pb.suspend(|| {
                    println!(
                        "{} {}",
                        "✔".green().bold(),
                        "Connected to database.".white()
                    )
                });
                execute_rollback(adapter, &pb).await?;
            }
            Err(e) => {
                pb.finish_and_clear();
                println!(
                    "\n{} {}\n",
                    "✖".red().bold(),
                    "Database connection failed.".bold()
                );
                println!("  {} {}", "Reason:".yellow().bold(), e.to_string().white());
            }
        },
        Database::Mysql => match MySqlAdapter::connect(url).await {
            Ok(adapter) => {
                pb.suspend(|| {
                    println!(
                        "{} {}",
                        "✔".green().bold(),
                        "Connected to database.".white()
                    )
                });

                execute_rollback(adapter, &pb).await?;
            }
            Err(e) => {
                pb.finish_and_clear();
                println!(
                    "\n{} {}\n",
                    "✖".red().bold(),
                    "Database connection failed.".bold()
                );
                println!("  {} {}", "Reason:".yellow().bold(), e.to_string().white());
            }
        },
        Database::Sqlite => match SqliteAdapter::connect(url).await {
            Ok(adapter) => {
                pb.suspend(|| {
                    println!(
                        "{} {}",
                        "✔".green().bold(),
                        "Connected to database.".white()
                    )
                });
                execute_rollback(adapter, &pb).await?;
            }
            Err(e) => {
                pb.finish_and_clear();
                println!(
                    "\n{} {}\n",
                    "✖".red().bold(),
                    "Database connection failed.".bold()
                );
                println!("  {} {}", "Reason:".yellow().bold(), e.to_string().white());
            }
        },
    }

    Ok(())
}

async fn execute_rollback<T>(adapter: T, pb: &ProgressBar) -> DinocoResult<()>
where
    T: DinocoAdapter + DinocoAdapterHandler + MigrationExecutor,
{
    pb.set_message("Fetching migration history...");

    let tables = adapter.fetch_tables().await?;
    let has_history_table = tables
        .iter()
        .any(|table| table.name == "_dinoco_migrations");

    if !has_history_table {
        pb.finish_and_clear();
        println!(
            "{} {}",
            "✔".green().bold(),
            "No migrations found to rollback.".white()
        );
        return Ok(());
    }

    let mut migrations = get_last_two_migrations(&adapter).await?;

    pb.finish_and_clear();

    if migrations.is_empty() {
        println!(
            "{} {}",
            "✔".green().bold(),
            "No migrations found to rollback.".white()
        );
        return Ok(());
    }

    let current_migration = migrations.remove(0);

    println!(
        "{} {}",
        "✔".green().bold(),
        format!("Found migration to rollback: '{}'.", current_migration.name).white()
    );

    match Confirm::new(&format!(
        "Are you sure you want to rollback '{}'?",
        current_migration.name
    ))
    .with_default(false)
    .prompt()
    {
        Ok(true) => {
            println!(
                "{} {}",
                "⚠".yellow().bold(),
                "Rolling back migration...".yellow()
            );
        }
        _ => {
            println!("{} {}", "✗".red().bold(), "Rollback cancelled.".white());
            return Ok(());
        }
    }

    if let Some(previous_migration) = migrations.first() {
        let current_schema = decode_schema(&current_migration.schema);
        let previous_schema = decode_schema(&previous_migration.schema);
        let rollback_plan = calculate_diff(&Some(current_schema), &previous_schema);
        let sqls = adapter.build_migration(&rollback_plan.steps, &previous_schema, false);

        for sql in sqls {
            adapter.execute(&sql, &[]).await?;
        }

        delete_migration(&adapter, &current_migration.name).await?;
        println!(
            "{} {}",
            "✔".green().bold(),
            "Rollback applied to database.".white()
        );
        println!(
            "{} {}",
            "✔".green().bold(),
            "Migration history updated.".white()
        );
        println!(
            "{} {}",
            "ℹ".blue(),
            "Local migration files were kept intact.".bright_black()
        );
    } else {
        adapter.reset_database().await?;
        create_migration_table(&adapter).await?;
        println!(
            "{} {}",
            "✔".green().bold(),
            "Database reset to an empty state.".white()
        );
        println!(
            "{} {}",
            "✔".green().bold(),
            "Migration history cleared.".white()
        );
        println!(
            "{} {}",
            "ℹ".blue(),
            "Local migration files were kept intact.".bright_black()
        );
    }

    println!(
        "{} {}",
        "✔".green().bold(),
        "Rollback completed successfully!".white()
    );

    Ok(())
}
