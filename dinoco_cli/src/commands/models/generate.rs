use std::env;
use std::fs;
use std::path::Path;

use colored::Colorize;

use dinoco_codegen::generate_models;
use dinoco_compiler::{ConnectionUrl, Database, ParsedConfig, ParsedSchema, compile, render_error};
use dinoco_engine::{DinocoAdapter, DinocoResult, MySqlAdapter, PostgresAdapter, SqliteAdapter};

use crate::{get_last_migration, local_migration_names, read_migration_schema};

pub async fn generate_models_from_latest_migration() -> DinocoResult<()> {
    let schema_path = "dinoco/schema.dinoco";

    if !Path::new(schema_path).exists() {
        println!("\n{} {}\n", "✖".red().bold(), "Dinoco project not initialized.".bold());

        return Ok(());
    }

    let source = match fs::read_to_string(schema_path) {
        Ok(content) => content,
        Err(err) => {
            println!("\n{} {}\n", "✖".red().bold(), "Failed to read schema file.".bold());
            println!("  {} {}", "Reason:".yellow().bold(), err.to_string().white());

            return Ok(());
        }
    };

    let parsed = match compile(&source) {
        Ok((_, parsed)) => parsed,
        Err(errs) => {
            println!("\n{} {}\n", "✖".red().bold(), "Schema compilation failed.".bold());

            for err in errs {
                println!("{}", render_error(&err, &source));
            }

            return Ok(());
        }
    };

    let (url, database) = match resolve_database_url(&parsed.config) {
        Some(result) => result,
        None => return Ok(()),
    };

    match database {
        Database::Postgresql => generate_models_from_database_state::<PostgresAdapter>(url, parsed.clone()).await?,
        Database::Mysql => generate_models_from_database_state::<MySqlAdapter>(url, parsed.clone()).await?,
        Database::Sqlite => generate_models_from_database_state::<SqliteAdapter>(url, parsed).await?,
    }

    Ok(())
}

async fn generate_models_from_database_state<T>(database_url: String, fallback_schema: ParsedSchema) -> DinocoResult<()>
where
    T: DinocoAdapter,
{
    let adapter = T::connect(database_url).await?;
    let last_migration = get_last_migration(&adapter).await?;

    let Some(migration) = last_migration else {
        println!("\n{} {}\n", "✖".red().bold(), "No migrations were found in the database.".bold());

        return Ok(());
    };

    let local_names = local_migration_names()?;

    if !local_names.iter().any(|name| name == &migration.name) {
        println!("\n{} {}\n", "✖".red().bold(), "The latest database migration was not found locally.".bold());
        println!("  {} {}", "→ Migration:".yellow().bold(), migration.name.cyan());

        return Ok(());
    }

    let parsed_schema = match read_migration_schema(&migration.name) {
        Ok(parsed_schema) => parsed_schema,
        Err(err) => {
            println!(
                "{} {}",
                "ℹ".blue(),
                format!(
                    "Failed to read the stored schema snapshot for '{}'. Falling back to the current schema.dinoco. Details: {}",
                    migration.name, err
                )
                .bright_black()
            );

            fallback_schema
        }
    };

    generate_models(parsed_schema);

    println!(
        "{} {}",
        "✔".green().bold(),
        "Rust models generated successfully from the latest migration stored in the database.".white()
    );

    Ok(())
}

fn resolve_database_url(config: &ParsedConfig) -> Option<(String, Database)> {
    let ParsedConfig { database, database_url, .. } = config;

    let url = match database_url {
        ConnectionUrl::Env(var_name) => match env::var(&var_name) {
            Ok(value) => value,
            Err(_) => {
                println!("\n{} {}\n", "✖".red().bold(), "Missing environment variable.".bold());
                println!("  {} {}", "→ Variable:".yellow().bold(), var_name.cyan());

                return None;
            }
        },
        ConnectionUrl::Literal(url) => url.clone(),
    };

    Some((url, database.clone()))
}
