use std::env;
use std::fs::{self, read_to_string};
use std::path::Path;
use std::time::Duration;

use colored::*;
use indicatif::ProgressBar;

use dinoco_compiler::{ConnectionUrl, Database, ParsedConfig, ParsedSchema, compile, render_error};
use dinoco_engine::{
    DinocoAdapter, DinocoAdapterHandler, DinocoResult, MySqlAdapter, PostgresAdapter, SqliteAdapter,
};

use crate::{create_migration_table, encode_schema, get_all_migrations, insert_migration};

pub async fn run_migrations() -> DinocoResult<()> {
    let schema_path = "dinoco/schema.dinoco";

    if !Path::new(schema_path).exists() {
        println!(
            "\n{} {}\n",
            "✖".red().bold(),
            "Dinoco project not initialized.".bold()
        );
        return Ok(());
    }

    println!(
        "{} {}",
        "✔".green().bold(),
        "Starting migrations run...".white()
    );

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
                execute_run(adapter, &pb, parsed).await?;
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
                execute_run(adapter, &pb, parsed).await?;
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
                execute_run(adapter, &pb, parsed).await?;
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

async fn execute_run<T>(
    adapter: T,
    pb: &ProgressBar,
    parsed_schema: ParsedSchema,
) -> DinocoResult<()>
where
    T: DinocoAdapter + DinocoAdapterHandler,
{
    pb.set_message("Checking migration history...");

    let tables = adapter.fetch_tables().await?;
    let has_history_table = tables
        .iter()
        .any(|table| table.name == "_dinoco_migrations");

    if !has_history_table {
        pb.set_message("Initializing migration history...");
        create_migration_table(&adapter).await?;
    }

    let applied_migrations = get_all_migrations(&adapter).await?;
    let applied_names: Vec<String> = applied_migrations
        .into_iter()
        .map(|migration| migration.name)
        .collect();

    let migrations_dir = Path::new("dinoco/migrations");
    if !migrations_dir.exists() {
        pb.finish_and_clear();
        println!(
            "{} {}",
            "✔".green().bold(),
            "No local migrations directory was found.".white()
        );
        return Ok(());
    }

    let mut local_folders = Vec::new();
    if let Ok(entries) = fs::read_dir(migrations_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if let Some(name) = path.file_name().and_then(|value| value.to_str()) {
                    local_folders.push(name.to_string());
                }
            }
        }
    }

    local_folders.sort();

    let pending: Vec<String> = local_folders
        .into_iter()
        .filter(|migration| !applied_names.contains(migration))
        .collect();

    pb.finish_and_clear();

    if pending.is_empty() {
        println!(
            "{} {}",
            "✔".green().bold(),
            "The database is already up to date.".white()
        );
        return Ok(());
    }

    println!(
        "{} {}",
        "✔".green().bold(),
        format!("Found {} pending migration(s).", pending.len()).white()
    );

    let schema_bytes = encode_schema(parsed_schema);

    for migration_name in pending {
        println!("  {} Applying '{}'...", "→".cyan().bold(), migration_name);

        let migration_dir = migrations_dir.join(&migration_name);
        let sql_path = migration_dir.join("migration.sql");

        let sql_content = match fs::read_to_string(&sql_path) {
            Ok(content) => content,
            Err(err) => {
                println!(
                    "    {} {}",
                    "✖".red(),
                    "Failed to read migration SQL.".bold()
                );
                println!("    {} {}", "Path:".yellow(), sql_path.display());
                println!("    {} {}", "Reason:".red(), err);

                return Ok(());
            }
        };

        for statement in sql_content.split(';') {
            let clean_statement = statement.trim();
            if clean_statement.is_empty() {
                continue;
            }

            if let Err(err) = adapter.execute(clean_statement, &[]).await {
                println!(
                    "    {} {} {}",
                    "✖".red(),
                    "Failed to execute migration:".bold(),
                    migration_name.yellow()
                );
                println!("    {} {}", "Statement:".yellow(), clean_statement.cyan());
                println!("    {} {}", "Reason:".red(), err);

                return Ok(());
            }
        }

        insert_migration(&adapter, &migration_name, schema_bytes.clone()).await?;
        println!("    {} Applied successfully.", "✔".green());
    }

    println!(
        "\n{} {}",
        "✔".green().bold(),
        "All local migrations were applied successfully!".white()
    );

    Ok(())
}
