use std::env;
use std::fs;
use std::path::{Component, Path, PathBuf};

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
    let schema = dinoco_compiler::compile_file(path).map_err(|err| anyhow!(err.to_string()))?;

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

/// Name of the folder, inside each workspace's migrations directory, that
/// holds the copy of the schema last used for that workspace.
pub const WORKSPACE_SCHEMA_DIR: &str = "schema";

/// Where `workspace`'s copy of the schema lives: `dinoco/migrations/<workspace>/schema/`.
pub fn workspace_schema_dir(workspace: &str) -> PathBuf {
    Path::new("dinoco/migrations").join(workspace).join(WORKSPACE_SCHEMA_DIR)
}

/// Copies `dinoco/schema.dinoco` and every file it imports into
/// `dinoco/migrations/<workspace>/schema/`, keeping their paths relative to
/// `dinoco/`, so the copy compiles on its own. The previous copy is replaced
/// as a whole, so files no longer imported disappear from it. Returns the
/// saved paths, relative to the copy's root.
pub fn save_workspace_schema(workspace: &str) -> anyhow::Result<Vec<PathBuf>> {
    let root = Path::new("dinoco/schema.dinoco");
    let (_, files) = dinoco_compiler::compile_file_with_sources(root).map_err(|err| anyhow!(err.to_string()))?;
    let schema_dir = fs::canonicalize("dinoco").context("dinoco/ was not found")?;

    let target = workspace_schema_dir(workspace);
    let staging = target.with_file_name(format!(".{WORKSPACE_SCHEMA_DIR}.saving"));
    if staging.exists() {
        fs::remove_dir_all(&staging)?;
    }

    let mut saved = files.iter().map(|file| snapshot_path(&schema_dir, file)).collect::<Vec<_>>();
    for (file, relative) in files.iter().zip(&saved) {
        let destination = staging.join(relative);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(file, &destination).with_context(|| format!("failed to save `{}`", file.display()))?;
    }

    if target.exists() {
        fs::remove_dir_all(&target)?;
    }
    fs::rename(&staging, &target)?;

    saved.sort();
    Ok(saved)
}

/// A schema file's place inside the copy: its path relative to `dinoco/`.
/// Files imported from outside `dinoco/` go under `_external/`, with each
/// `..` spelled `_up` so they stay inside the copy.
fn snapshot_path(schema_dir: &Path, file: &Path) -> PathBuf {
    if let Ok(relative) = file.strip_prefix(schema_dir) {
        return relative.to_path_buf();
    }

    let mut common = schema_dir.to_path_buf();
    let mut ups = 0;
    while !file.starts_with(&common) && common.pop() {
        ups += 1;
    }
    let mut path = PathBuf::from("_external");
    for _ in 0..ups {
        path.push("_up");
    }
    for component in file.strip_prefix(&common).unwrap_or(file).components() {
        if let Component::Normal(part) = component {
            path.push(part);
        }
    }
    path
}
