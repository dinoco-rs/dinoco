use std::fs;
use std::path::Path;

use crate::error::CliError;
use crate::prompt::{self, Choice};
use crate::ui;

/// Answers to `dinoco init`'s questions given on the command line.
#[derive(Debug, Default, Clone)]
pub struct InitOptions {
    pub database: Option<&'static str>,
    pub connection: Option<&'static str>,
}

const DATABASES: [(&str, &str); 3] =
    [("postgresql", "PostgreSQL"), ("mysql", "MySQL"), ("sqlite", "SQLite (a local database file)")];

const CONNECTIONS: [(&str, &str); 2] = [
    ("direct", "Direct: connect to PostgreSQL itself"),
    ("pgbouncer", "PgBouncer: connect through a transaction-pooling proxy"),
];

pub fn run(options: InitOptions) -> anyhow::Result<()> {
    let database = match options.database.or_else(|| env_choice("DINOCO_CLI_INIT_DATABASE", &DATABASES)) {
        Some(database) => database,
        None if prompt::is_interactive() => {
            prompt::select("Which database will this project use?", choices(&DATABASES), 0)?
        }
        None => {
            return Err(CliError::decision_required("A database must be selected to initialize the project.")
                .detail("Options", "postgresql, mysql, sqlite")
                .hint("Pass --database <postgresql|mysql|sqlite>.")
                .into());
        }
    };

    let connection = if database == "postgresql" {
        Some(match options.connection.or_else(|| env_choice("DINOCO_CLI_INIT_POSTGRES_CONNECTION", &CONNECTIONS)) {
            Some(connection) => connection,
            None if prompt::is_interactive() => {
                prompt::select("How should Dinoco connect to PostgreSQL?", choices(&CONNECTIONS), 0)?
            }
            // `direct` is what a schema without `connection` uses.
            None => "direct",
        })
    } else {
        if options.connection.is_some() {
            return Err(CliError::usage("--connection only applies to PostgreSQL projects.")
                .detail("Database", database)
                .hint("Remove --connection, or pass --database postgresql.")
                .into());
        }
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
    }

    ui::heading("Project Setup");
    let mut fields = vec![("Database", database_label(database).to_string())];
    if let Some(connection) = connection {
        fields.push(("Connection", connection.to_string()));
    }
    ui::fields(&fields);
    ui::blank();
    if schema_created {
        ui::success("Dinoco project initialized");
        ui::success("Created dinoco/schema.dinoco");
    } else {
        ui::info("dinoco/schema.dinoco already exists and was left unchanged.");
    }
    if migrations_dir_created {
        ui::success("Created dinoco/migrations/");
    }

    ui::heading("Next steps");
    ui::line("1. Set DATABASE_URL in your environment or in a .env file");
    ui::line("2. Define your models in dinoco/schema.dinoco");
    ui::line("3. Run `dinoco migrate generate` to create your first migration");

    ui::docs("/docs/orm/guide/introduction");

    Ok(())
}

fn choices(options: &[(&'static str, &'static str)]) -> Vec<Choice<&'static str>> {
    options.iter().map(|(value, label)| Choice::new(*label, *value)).collect()
}

fn env_choice(key: &str, options: &[(&'static str, &'static str)]) -> Option<&'static str> {
    let value = std::env::var(key).ok()?;
    options.iter().map(|(option, _)| *option).find(|option| *option == value)
}

fn database_label(database: &str) -> &'static str {
    match database {
        "postgresql" => "PostgreSQL",
        "mysql" => "MySQL",
        _ => "SQLite",
    }
}
