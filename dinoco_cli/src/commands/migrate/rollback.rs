use std::env;
use std::fs::read_to_string;
use std::path::Path;
use std::time::Duration;

use colored::*;
use indicatif::ProgressBar;
use inquire::Confirm;

use dinoco_compiler::{ConnectionUrl, Database, ParsedConfig, compile, render_error};
use dinoco_engine::{DinocoAdapter, DinocoResult, Migration, MySqlAdapter, PostgresAdapter, SqlDialectBuilders};

use crate::helpers::{decode_schema, delete_migration, drop_all_tables, fetch, get_last_two_migrations};

pub async fn rollback_migration() -> DinocoResult<()> {
    let schema_path = "dinoco/schema.dinoco";

    if !Path::new(schema_path).exists() {
        println!("\n{} {}\n", "✖".red().bold(), "Dinoco project not initialized.".bold());
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
                execute_rollback(adapter, &pb).await?;
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
                execute_rollback(adapter, &pb).await?;
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

async fn execute_rollback<T>(adapter: T, pb: &ProgressBar) -> DinocoResult<()>
where
    T: DinocoAdapter,
    T::Dialect: SqlDialectBuilders,
{
    pb.set_message("Fetching migration history...");

    let mut migrations = get_last_two_migrations(&adapter).await?;

    pb.finish_and_clear();

    if migrations.is_empty() {
        println!("{} {}", "✔".green().bold(), "No migrations found to rollback.".white());
        return Ok(());
    }

    let target_migration = migrations.remove(0);

    println!("{} {}", "✔".green().bold(), format!("Found migration to rollback: '{}'.", target_migration.name).white());

    let confirm = Confirm::new(&format!("Are you sure you want to rollback '{}'?", target_migration.name))
        .with_default(false)
        .prompt();

    match confirm {
        Ok(true) => {
            println!("{} {}", "⚠".yellow().bold(), "Rolling back migration...".yellow());
        }
        _ => {
            println!("{} {}", "✗".red().bold(), "Rollback cancelled.".white());

            return Ok(());
        }
    }

    if migrations.len() > 0 {
        let current_schema = decode_schema(&target_migration.schema);

        let target_rollback_schema = if !migrations.is_empty() {
            decode_schema(&migrations[0].schema)
        } else {
            let mut empty_schema = current_schema.clone();
            empty_schema.tables.clear();
            empty_schema
        };

        let reverse_engine = Migration::new(&adapter, Some(current_schema), target_rollback_schema);
        let reverse_changes = reverse_engine.diff();

        println!("{} {}", "✔".green().bold(), format!("Detected {} rollback step(s).", reverse_changes.len()).white());

        for sql in reverse_engine.to_up_sql(reverse_changes) {
            adapter.execute(&sql, &[]).await?;
        }

        delete_migration(&adapter, target_migration.id).await?;
    } else {
        let tables = fetch(&adapter).await?;

        drop_all_tables(&adapter, tables).await?;
    }

    println!("{} {}", "✔".green().bold(), "Rollback applied to database.".white());

    let migration_dir = if migrations.len() > 0 {
        format!("dinoco/migrations/{}", target_migration.name)
    } else {
        "dinoco/migrations".to_string()
    };

    println!("{}", Path::new(&migration_dir).exists());

    if Path::new(&migration_dir).exists() {
        std::fs::remove_dir_all(&migration_dir).unwrap_or_default();
    }

    println!("{} {}", "✔".green().bold(), "Migration history updated.".white());

    println!("{} {}", "✔".green().bold(), "Rollback completed successfully!".white());

    Ok(())
}
