use std::env;
use std::fs;
use std::path::Path;

use anyhow::{Context, anyhow};
use dinoco_compiler::{ConfigValue, Schema};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Database {
    Postgresql,
    Mysql,
    Sqlite,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PostgresConnection {
    Direct,
    PgBouncer,
}

#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    pub database: Database,
    pub postgres_connection: PostgresConnection,
    pub database_url: String,
}

pub fn read_schema() -> anyhow::Result<(String, Schema)> {
    let path = Path::new("dinoco/schema.dinoco");
    let source = fs::read_to_string(path).context("dinoco/schema.dinoco was not found. Run `dinoco init`.")?;
    let schema = dinoco_compiler::compile(&source).map_err(|err| anyhow!(err.to_string()))?;

    validate_schema_relations(&schema)?;

    Ok((source, schema))
}

pub fn runtime_config(schema: &Schema) -> anyhow::Result<RuntimeConfig> {
    let config = schema.config().context("config block was not found in the schema")?;
    let database = config
        .entries
        .iter()
        .find(|entry| entry.key == "database")
        .and_then(|entry| match &entry.value {
            ConfigValue::String(value) | ConfigValue::Ident(value) => Some(value.as_str()),
            _ => None,
        })
        .context("config.database was not found")?;
    let database = match database {
        "postgresql" | "postgres" => Database::Postgresql,
        "mysql" => Database::Mysql,
        "sqlite" => Database::Sqlite,
        other => return Err(anyhow!("database `{other}` is not supported")),
    };
    let postgres_connection = config
        .entries
        .iter()
        .find(|entry| entry.key == "connection")
        .and_then(|entry| match &entry.value {
            ConfigValue::String(value) | ConfigValue::Ident(value) if value == "pgbouncer" => {
                Some(PostgresConnection::PgBouncer)
            }
            ConfigValue::String(_) | ConfigValue::Ident(_) => Some(PostgresConnection::Direct),
            _ => None,
        })
        .unwrap_or(PostgresConnection::Direct);
    let database_url_env = config
        .entries
        .iter()
        .find(|entry| entry.key == "database_url")
        .and_then(|entry| match &entry.value {
            ConfigValue::Env(value) => Some(value),
            _ => None,
        })
        .context("config.database_url must be env(\"DATABASE_URL\")")?;
    let database_url = env::var(database_url_env).with_context(|| format!("env `{database_url_env}` was not found"))?;

    Ok(RuntimeConfig { database, postgres_connection, database_url })
}

pub fn validate_schema_relations(schema: &Schema) -> anyhow::Result<()> {
    let models = schema.models().map(|model| model.name.as_str()).collect::<Vec<_>>();
    let enums = schema.enums().map(|item| item.name.as_str()).collect::<Vec<_>>();
    let scalars = ["String", "Boolean", "Integer", "Float", "DateTime", "Date", "Json"];

    for model in schema.models() {
        for field in &model.fields {
            let known = scalars.contains(&field.ty.name.as_str())
                || models.contains(&field.ty.name.as_str())
                || enums.contains(&field.ty.name.as_str());
            if !known {
                return Err(anyhow!(
                    "type `{}` was not found for field `{}.{}`",
                    field.ty.name,
                    model.name,
                    field.name
                ));
            }

            if field.attributes.iter().any(|attr| attr.name == "relation") && !models.contains(&field.ty.name.as_str())
            {
                return Err(anyhow!("relation `{}.{}` points to a model that does not exist", model.name, field.name));
            }
        }
    }

    Ok(())
}
