use std::fs;
use std::path::Path;
use std::time::Duration;
use std::{env, fs::read_to_string};

use colored::*;
use indicatif::ProgressBar;

use dinoco_compiler::{ConnectionUrl, ParsedConfig, ParsedField, Relation, compile, render_error};
use dinoco_engine::{DinocoAdapter, Migration, PostgresAdapter};

pub async fn migrate_create() {
    let schema_path = "dinoco/schema.dinoco";

    if !Path::new(schema_path).exists() {
        println!("\n{} {}\n", "✖".red().bold(), "Dinoco project not initialized.".bold());
        println!("{} {}", "→ Missing schema file:".yellow().bold(), schema_path.cyan());
        println!("\n{} {}\n", "Hint:".blue().bold(), "Run the command below to initialize your project:".white());
        println!("  {} {}\n", "dinoco init".green().bold(), "(creates the required project structure)".dimmed());
        return;
    }

    println!("{} {}", "✔".green().bold(), "Schema found. Starting migration...".white());

    let pb = ProgressBar::new_spinner();
    pb.set_message("Compiling schema...");
    pb.enable_steady_tick(Duration::from_millis(80));

    let source = match read_to_string(schema_path) {
        Ok(content) => content,
        Err(e) => {
            pb.finish_and_clear();

            println!("\n{} {}\n", "✖".red().bold(), "Failed to read schema file.".bold());
            println!("{} {}", "Reason:".yellow().bold(), e.to_string().white());
            return;
        }
    };

    match compile(&source) {
        Ok((_, parsed)) => {
            pb.finish_and_clear();

            println!("{} {}", "✔".green().bold(), "Schema compiled successfully.".white());

            let ParsedConfig { database, database_url, .. } = &parsed.config;

            let url = match database_url {
                ConnectionUrl::Env(var_name) => match env::var(&var_name) {
                    Ok(val) => val,
                    Err(_) => {
                        println!("\n{} {}\n", "✖".red().bold(), "Missing environment variable.".bold());
                        println!("{} {}", "→ Variable:".yellow().bold(), var_name.cyan());
                        println!("{} {}", "Hint:".blue().bold(), format!("Define {} in your environment or .env file.", var_name).white());
                        return;
                    }
                },
                ConnectionUrl::Literal(url) => url.clone(),
            };

            pb.set_message(format!("{} {}", "→".cyan().bold(), format!("Connecting to {:?} database...", database).white()));

            let adapter = PostgresAdapter::connect(url).await;

            if let Err(e) = &adapter {
                pb.finish_and_clear();

                println!("\n{} {}\n", "✖".red().bold(), "Database connection failed.".bold());
                println!("{} {}", "Reason:".yellow().bold(), e.to_string().white());
                return;
            }

            pb.finish_and_clear();

            println!("{} {}", "✔".green().bold(), "Connected to database.".white());

            pb.set_message("Detecting schema changes...");
            pb.enable_steady_tick(Duration::from_millis(80));

            let (_, old_schema) = compile(&read_to_string("dinoco/old.dinoco").unwrap()).unwrap();

            let migration = Migration::new(adapter.unwrap(), Some(old_schema), parsed);
            let changes = migration.diff();

            pb.finish_and_clear();

            if changes.is_empty() {
                println!("{} {}", "✔".green().bold(), "No changes detected.".white());
                println!("{} {}", "ℹ".blue().bold(), "Your schema is already up to date.".dimmed());
                return;
            }

            println!("{} {}", "✔".green().bold(), format!("Detected {} change(s).", changes.len()).white());

            let sql = migration.to_up_sql(changes);

            std::fs::create_dir_all("dinoco/migrations/teste").unwrap();
            std::fs::write("dinoco/migrations/teste/migration.sql", &sql).unwrap();

            println!("{} {}", "→".cyan().bold(), "Migration file generated successfully.".white());
        }

        Err(errs) => {
            pb.finish_and_clear();

            println!("\n{} {}\n", "✖".red().bold(), format!("Schema compilation failed ({} error(s)).", errs.len()).bold());

            for err in errs {
                println!("{}", render_error(&err, &source));
            }

            println!("\n{} {}\n", "Hint:".blue().bold(), "Fix the errors above and run the command again.".white());
        }
    }

    println!("{} {}", "✔".green().bold(), "Migration completed.".white());
}
