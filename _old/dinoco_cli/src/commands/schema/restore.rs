use std::fs;

use colored::Colorize;

use dinoco_codegen::dinoco::render_schema;
use dinoco_engine::DinocoResult;
use dinoco_formatter::format_from_raw;

use crate::{latest_local_migration_name, local_migration_names, read_migration_schema};

pub fn restore_schema_from_migration(migration_name: Option<&str>) -> DinocoResult<()> {
    let selected_migration = match migration_name {
        Some(name) => {
            let local_names = local_migration_names()?;

            if !local_names.iter().any(|migration| migration == name) {
                println!("\n{} {}\n", "✖".red().bold(), "The requested migration was not found locally.".bold());
                println!("  {} {}", "→ Migration:".yellow().bold(), name.cyan());

                return Ok(());
            }

            name.to_string()
        }
        None => {
            let Some(name) = latest_local_migration_name()? else {
                println!("\n{} {}\n", "✖".red().bold(), "No local migrations were found.".bold());
                println!(
                    "  {} {}",
                    "Hint:".blue().bold(),
                    "Generate a migration first so Dinoco can restore the schema from schema.bin.".white()
                );

                return Ok(());
            };

            name
        }
    };

    let parsed_schema = read_migration_schema(&selected_migration)?;
    let raw_schema = render_schema(&parsed_schema);
    let formatted_schema = format_from_raw(&raw_schema).unwrap_or(raw_schema);

    fs::create_dir_all("dinoco")?;
    fs::write("dinoco/schema.dinoco", formatted_schema)?;

    println!("{} {}", "✔".green().bold(), "schema.dinoco was restored successfully.".white());
    println!("  {} {} {}", "→".cyan().bold(), "Source migration:".dimmed(), selected_migration.cyan());
    println!("  {} {} {}", "→".cyan().bold(), "Restored file:".dimmed(), "dinoco/schema.dinoco".cyan());

    Ok(())
}
