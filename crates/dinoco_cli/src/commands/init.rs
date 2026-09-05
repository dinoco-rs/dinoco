use std::fs;
use std::path::Path;

use inquire::Select;

use crate::ui;

pub fn run() -> anyhow::Result<()> {
    let database = env_or_select(
        "DINOCO_CLI_INIT_DATABASE",
        "Which database do you want to use?",
        vec!["postgresql", "mysql", "sqlite"],
    )?;
    let connection = if database == "postgresql" {
        Some(env_or_select(
            "DINOCO_CLI_INIT_POSTGRES_CONNECTION",
            "How should Dinoco connect to Postgres?",
            vec!["direct", "pgbouncer"],
        )?)
    } else {
        None
    };

    let migrations_dir = Path::new("dinoco/migrations");
    let migrations_dir_created = !migrations_dir.exists();
    fs::create_dir_all(migrations_dir)?;

    let mut schema = String::new();
    schema.push_str("config {\n");
    schema.push_str(&format!("    database = \"{database}\"\n"));
    if let Some(connection) = connection {
        schema.push_str(&format!("    connection = \"{connection}\"\n"));
    }
    schema.push_str("    database_url = env(\"DATABASE_URL\")\n");
    schema.push_str("    read_replicas = []\n");
    schema.push_str("}\n");

    let schema = dinoco_formatter::format_from_raw(&schema)?;
    let schema_path = Path::new("dinoco/schema.dinoco");
    let schema_created = !schema_path.exists();
    if schema_created {
        fs::write(schema_path, schema)?;
        ui::success("Dinoco project initialized");
    } else {
        ui::warning("dinoco/schema.dinoco already exists; keeping the current file");
    }

    println!();
    if schema_created {
        ui::info("Created dinoco/schema.dinoco");
    }
    if migrations_dir_created {
        ui::info("Created dinoco/migrations/");
    }

    println!("\nNext steps:");
    println!("  1. Set DATABASE_URL in your environment");
    println!("  2. Define your models in dinoco/schema.dinoco");
    println!("  3. Run `dinoco migrate generate` to create your first migration");

    ui::docs("/getting-started");

    Ok(())
}

fn env_or_select(key: &str, prompt: &str, options: Vec<&'static str>) -> anyhow::Result<&'static str> {
    if let Ok(value) = std::env::var(key)
        && let Some(option) = options.iter().copied().find(|option| *option == value)
    {
        return Ok(option);
    }

    Ok(Select::new(prompt, options).prompt()?)
}
