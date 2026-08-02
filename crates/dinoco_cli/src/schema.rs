use std::env;
use std::fs;
use std::path::Path;

use anyhow::{Context, anyhow};
use dinoco_compiler::{ConfigValue, Schema};
use inquire::Select;

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
    pub min_connection: usize,
    pub max_connection: usize,
}

pub fn read_schema() -> anyhow::Result<(String, Schema)> {
    let path = Path::new("dinoco/schema.dinoco");
    let source = fs::read_to_string(path).context("dinoco/schema.dinoco was not found. Run `dinoco init`.")?;
    let schema = dinoco_compiler::compile(&source).map_err(|err| anyhow!(err.to_string()))?;

    validate_schema_relations(&schema)?;

    Ok((source, schema))
}

pub fn read_schema_for_workspace(requested: Option<&str>) -> anyhow::Result<(String, Schema, Option<String>)> {
    let (source, schema) = read_schema()?;
    let workspace_names = schema.workspaces().map(|workspace| workspace.name.clone()).collect::<Vec<_>>();

    if workspace_names.is_empty() {
        if let Some(requested) = requested {
            anyhow::bail!("Workspace `{requested}` was requested, but schema.dinoco does not configure workspaces");
        }
        return Ok((source, schema, None));
    }

    let workspace = match requested {
        Some(name) if workspace_names.iter().any(|candidate| candidate == name) => name.to_string(),
        Some(name) => {
            anyhow::bail!("Workspace `{name}` was not found. Available workspaces: {}", workspace_names.join(", "))
        }
        None => Select::new("Which workspace do you want to use?", workspace_names).prompt()?,
    };
    let selected = schema
        .for_workspace(&workspace)
        .with_context(|| format!("failed to load workspace `{workspace}` from schema.dinoco"))?;

    Ok((source, selected, Some(workspace)))
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
    let min_connection = config_integer(config, "min_connection").unwrap_or(2);
    let max_connection = config_integer(config, "max_connection").unwrap_or(10);

    Ok(RuntimeConfig { database, postgres_connection, database_url, min_connection, max_connection })
}

fn config_integer(config: &dinoco_compiler::ConfigBlock, key: &str) -> Option<usize> {
    config.entries.iter().find(|entry| entry.key == key).and_then(|entry| match &entry.value {
        ConfigValue::Integer(value) if *value > 0 => usize::try_from(*value).ok(),
        _ => None,
    })
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
