use std::env;
use std::fs;
use std::io::ErrorKind as IoErrorKind;
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, anyhow};
use dinoco_compiler::{ConfigValue, Schema, SchemaItem};

use crate::error::{CliError, WithCliError};
use crate::prompt::{self, Choice};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Database {
    Postgresql,
    Mysql,
    Sqlite,
}

impl Database {
    pub fn label(self) -> &'static str {
        match self {
            Database::Postgresql => "PostgreSQL",
            Database::Mysql => "MySQL",
            Database::Sqlite => "SQLite",
        }
    }

    /// Longest index or constraint name the database keeps.
    pub fn identifier_limit(self) -> Option<dinoco_compiler::IdentifierLimit> {
        match self {
            Database::Postgresql => Some(dinoco_compiler::IdentifierLimit::POSTGRES),
            Database::Mysql => Some(dinoco_compiler::IdentifierLimit::MYSQL),
            Database::Sqlite => None,
        }
    }
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

impl RuntimeConfig {
    /// The database and, for PostgreSQL, how it is reached.
    pub fn database_label(&self) -> &'static str {
        match (self.database, self.postgres_connection) {
            (Database::Postgresql, PostgresConnection::PgBouncer) => "PostgreSQL (through PgBouncer)",
            (database, _) => database.label(),
        }
    }
}

const SCHEMA_PATH: &str = "dinoco/schema.dinoco";

pub fn read_schema() -> anyhow::Result<(String, Schema)> {
    let path = Path::new(SCHEMA_PATH);
    let source = match fs::read_to_string(path) {
        Ok(source) => source,
        Err(error) if error.kind() == IoErrorKind::NotFound => {
            return Err(CliError::failure("dinoco/schema.dinoco was not found in the current directory.")
                .hint("Run `dinoco init` to create a project, or run the command from the project root.")
                .into());
        }
        Err(error) => {
            return Err(error).cli_error(|| CliError::failure("dinoco/schema.dinoco could not be read."));
        }
    };
    let schema = dinoco_compiler::compile_file(path).map_err(|err| anyhow!(err.to_string())).cli_error(|| {
        CliError::failure("dinoco/schema.dinoco has errors.").hint("Fix the errors below, then run the command again.")
    })?;

    validate_schema_relations(&schema).cli_error(|| {
        CliError::failure("dinoco/schema.dinoco has errors.").hint("Fix the errors below, then run the command again.")
    })?;

    Ok((source, schema))
}

/// What a command asks when the schema defines more than one workspace.
#[derive(Debug, Clone, Copy)]
pub struct Questions {
    pub workspace: &'static str,
    /// Asked after the workspace, to pick the schema to use for it. `None`
    /// for commands that do not read the models, such as `migrate run`.
    pub schema: Option<&'static str>,
}

/// Where the models and enums a command uses come from. The config, and so
/// the database, always comes from `dinoco/schema.dinoco`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SchemaSource {
    /// `dinoco/schema.dinoco`, the schema you edit.
    Current,
    /// The copy saved for a workspace in `dinoco/migrations/<workspace>/schema/`.
    Saved(String),
}

impl SchemaSource {
    /// Where the schema is read from, for the Target section.
    pub fn label(&self) -> String {
        match self {
            SchemaSource::Current => SCHEMA_PATH.to_string(),
            SchemaSource::Saved(workspace) => {
                format!("{}/ (saved for {workspace})", workspace_schema_dir(workspace).display())
            }
        }
    }
}

/// The schema a command works with, after the workspace and the schema
/// source were selected.
pub struct SelectedSchema {
    pub schema: Schema,
    pub workspace: Option<String>,
    pub source: SchemaSource,
}

/// Reads the schema and selects the workspace to use: `requested`, the only
/// one defined, or the one the user picks. Without a terminal and with more
/// than one workspace, `--workspace` is required.
pub fn read_schema_for_workspace(
    requested: Option<&str>,
    question: &str,
) -> anyhow::Result<(String, Schema, Option<String>)> {
    let (source, schema) = read_schema()?;
    let workspace = select_workspace(&schema, requested, question)?;
    let selected = match &workspace {
        Some(workspace) => schema
            .for_workspace(workspace)
            .with_context(|| format!("failed to load workspace `{workspace}` from schema.dinoco"))?,
        None => schema,
    };

    Ok((source, selected, workspace))
}

/// Selects the workspace like [`read_schema_for_workspace`], then the schema
/// to use for it: `schema_from`, or the one the user picks when more than one
/// workspace is defined and another workspace has a saved schema. Without a
/// terminal and without `schema_from`, `dinoco/schema.dinoco` is used.
///
/// A saved schema provides the models and enums; the selected workspace's
/// config still comes from `dinoco/schema.dinoco`.
pub fn select_schema(
    requested: Option<&str>,
    schema_from: Option<&str>,
    questions: Questions,
) -> anyhow::Result<SelectedSchema> {
    let (_, schema) = read_schema()?;
    let workspace = select_workspace(&schema, requested, questions.workspace)?;
    let Some(workspace) = workspace else {
        if schema_from.is_some() {
            return Err(CliError::usage(
                "--schema-from was passed, but dinoco/schema.dinoco does not define workspaces.",
            )
            .hint("Remove --schema-from. Without workspaces, dinoco/schema.dinoco is the only schema.")
            .into());
        }
        return Ok(SelectedSchema { schema, workspace: None, source: SchemaSource::Current });
    };

    let source = select_schema_source(&schema, &workspace, schema_from, questions.schema)?;
    let current = schema
        .for_workspace(&workspace)
        .with_context(|| format!("failed to load workspace `{workspace}` from schema.dinoco"))?;
    let schema = match &source {
        SchemaSource::Current => current,
        SchemaSource::Saved(from) => read_saved_schema(from, current)?,
    };

    Ok(SelectedSchema { schema, workspace: Some(workspace), source })
}

fn select_schema_source(
    schema: &Schema,
    workspace: &str,
    requested: Option<&str>,
    question: Option<&str>,
) -> anyhow::Result<SchemaSource> {
    let workspaces = schema.workspaces().collect::<Vec<_>>();
    if let Some(name) = requested {
        if !workspaces.iter().any(|workspace| workspace.name == name) {
            return Err(CliError::usage(format!("Workspace `{name}` is not defined in dinoco/schema.dinoco."))
                .detail("Available", workspaces.iter().map(|item| item.name.as_str()).collect::<Vec<_>>().join(", "))
                .hint("Pass --schema-from with one of the defined workspaces.")
                .into());
        }
        if !has_saved_schema(name) {
            return Err(CliError::usage(format!("Workspace `{name}` has no saved schema yet."))
                .detail("Expected", workspace_schema_dir(name).join("schema.dinoco").display().to_string())
                .reason("A workspace's schema is saved when `migrate generate` or `models generate` runs for it.")
                .hint(format!(
                    "Run `{}` first, or remove --schema-from to use dinoco/schema.dinoco.",
                    crate::commands::command_line("migrate generate", Some(name))
                ))
                .into());
        }
        return Ok(SchemaSource::Saved(name.to_string()));
    }

    let Some(question) = question else {
        return Ok(SchemaSource::Current);
    };
    // The selected workspace's own copy is what it already has, so only the
    // other workspaces' copies are offered next to dinoco/schema.dinoco.
    let saved = workspaces
        .iter()
        .map(|other| other.name.as_str())
        .filter(|&other| other != workspace && has_saved_schema(other))
        .collect::<Vec<_>>();
    if saved.is_empty() || !prompt::is_interactive() {
        return Ok(SchemaSource::Current);
    }

    let current_files = current_schema_files().ok();
    let mut choices = vec![Choice::new(format!("{SCHEMA_PATH}  (the schema you edit)"), SchemaSource::Current)];
    for other in saved {
        let same = current_files.as_deref().is_some_and(|files| saved_schema_matches(other, files));
        let note = if same { ", same as dinoco/schema.dinoco" } else { "" };
        choices.push(Choice::new(
            format!("{other}'s schema  ({}/{note})", workspace_schema_dir(other).display()),
            SchemaSource::Saved(other.to_string()),
        ));
    }
    Ok(prompt::select(question, choices, 0)?)
}

fn has_saved_schema(workspace: &str) -> bool {
    workspace_schema_dir(workspace).join("schema.dinoco").is_file()
}

/// Compiles the schema saved for `workspace` and gives it the config of
/// `current`, the selected workspace in `dinoco/schema.dinoco`.
fn read_saved_schema(workspace: &str, current: Schema) -> anyhow::Result<Schema> {
    let path = workspace_schema_dir(workspace).join("schema.dinoco");
    let failure = || {
        CliError::failure(format!("The schema saved for `{workspace}` has errors."))
            .detail("Schema", path.display().to_string())
            .hint("Fix the errors below, or use dinoco/schema.dinoco instead.")
    };
    let mut saved = dinoco_compiler::compile_file(&path).map_err(|err| anyhow!(err.to_string())).cli_error(failure)?;
    validate_schema_relations(&saved).cli_error(failure)?;

    let config = current.items.into_iter().find(|item| matches!(item, SchemaItem::Config(_)));
    saved.items.retain(|item| !matches!(item, SchemaItem::Config(_)));
    if let Some(config) = config {
        saved.items.insert(0, config);
    }
    Ok(saved)
}

fn select_workspace(schema: &Schema, requested: Option<&str>, question: &str) -> anyhow::Result<Option<String>> {
    let workspaces = schema.workspaces().collect::<Vec<_>>();
    let names = || workspaces.iter().map(|workspace| workspace.name.as_str()).collect::<Vec<_>>().join(", ");

    if workspaces.is_empty() {
        if let Some(requested) = requested {
            return Err(CliError::usage(
                "--workspace was passed, but dinoco/schema.dinoco does not define workspaces.",
            )
            .detail("Workspace", requested)
            .hint("Remove --workspace, or define workspaces in the config block.")
            .into());
        }
        return Ok(None);
    }

    let workspace = match requested {
        Some(name) if workspaces.iter().any(|workspace| workspace.name == name) => name.to_string(),
        Some(name) => {
            return Err(CliError::usage(format!("Workspace `{name}` is not defined in dinoco/schema.dinoco."))
                .detail("Available", names())
                .hint(format!("Pass one of the defined workspaces, for example --workspace {}.", workspaces[0].name))
                .into());
        }
        // A single workspace is not a choice.
        None if workspaces.len() == 1 => workspaces[0].name.clone(),
        None if prompt::is_interactive() => {
            let choices = workspaces
                .iter()
                .map(|workspace| {
                    let label = match workspace_database_url_env(&workspace.entries) {
                        Some(env) => format!("{}  (database URL from {env})", workspace.name),
                        None => workspace.name.clone(),
                    };
                    Choice::new(label, workspace.name.clone())
                })
                .collect();
            prompt::select(question, choices, 0)?
        }
        None => {
            return Err(CliError::decision_required("A workspace must be selected.")
                .detail("Available", names())
                .reason("dinoco/schema.dinoco defines more than one workspace, and each one targets its own database.")
                .hint(format!("Pass --workspace <name>, for example --workspace {}.", workspaces[0].name))
                .into());
        }
    };

    Ok(Some(workspace))
}

fn workspace_database_url_env(entries: &[dinoco_compiler::ConfigEntry]) -> Option<&str> {
    entries.iter().find(|entry| entry.key == "database_url").and_then(|entry| match &entry.value {
        ConfigValue::Env(value) => Some(value.as_str()),
        _ => None,
    })
}

pub fn runtime_config(schema: &Schema) -> anyhow::Result<RuntimeConfig> {
    let config = schema.config().ok_or_else(|| {
        CliError::failure("dinoco/schema.dinoco has no config block.").hint(
            "Add a config block that sets `database` and `database_url`. `dinoco init` writes one in new projects.",
        )
    })?;
    let database = config
        .entries
        .iter()
        .find(|entry| entry.key == "database")
        .and_then(|entry| match &entry.value {
            ConfigValue::String(value) | ConfigValue::Ident(value) => Some(value.as_str()),
            _ => None,
        })
        .ok_or_else(|| {
            CliError::failure("The config block does not set `database`.")
                .hint("Add database = \"postgresql\", \"mysql\" or \"sqlite\" to the config block.")
        })?;
    let database = match database {
        "postgresql" | "postgres" => Database::Postgresql,
        "mysql" => Database::Mysql,
        "sqlite" => Database::Sqlite,
        other => {
            return Err(CliError::failure(format!("Database `{other}` is not supported."))
                .detail("Supported", "postgresql, mysql, sqlite")
                .hint("Set `database` in the config block to one of the supported databases.")
                .into());
        }
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
    let database_url_env = workspace_database_url_env(&config.entries).ok_or_else(|| {
        CliError::failure("The config block must read `database_url` from an environment variable.")
            .hint("Write database_url = env(\"DATABASE_URL\") in the config block.")
    })?;
    let database_url = env::var(database_url_env).map_err(|_| {
        CliError::failure(format!("The environment variable `{database_url_env}` is not set."))
            .reason(format!("The config block reads the {} connection URL from it.", database.label()))
            .hint(format!("Set {database_url_env} in your environment or in a .env file in the project root."))
    })?;
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

/// Saves the schema `workspace` was generated from into
/// `dinoco/migrations/<workspace>/schema/`.
///
/// From [`SchemaSource::Current`], it copies `dinoco/schema.dinoco` and every
/// file it imports, keeping their paths relative to `dinoco/`, so the copy
/// compiles on its own. From another workspace's saved schema, it copies that
/// folder. The previous copy is replaced as a whole, so files no longer
/// imported disappear from it. Returns the saved paths, relative to the
/// copy's root.
pub fn save_workspace_schema(workspace: &str, source: &SchemaSource) -> anyhow::Result<Vec<PathBuf>> {
    let files = match source {
        SchemaSource::Current => current_schema_files()?,
        // The copy is already the schema that was used.
        SchemaSource::Saved(from) if from == workspace => {
            let mut saved = saved_schema_files(workspace)?;
            saved.sort();
            return Ok(saved);
        }
        SchemaSource::Saved(from) => {
            let dir = workspace_schema_dir(from);
            saved_schema_files(from)?.into_iter().map(|relative| (dir.join(&relative), relative)).collect()
        }
    };

    let target = workspace_schema_dir(workspace);
    let staging = target.with_file_name(format!(".{WORKSPACE_SCHEMA_DIR}.saving"));
    if staging.exists() {
        fs::remove_dir_all(&staging)?;
    }

    for (file, relative) in &files {
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

    let mut saved = files.into_iter().map(|(_, relative)| relative).collect::<Vec<_>>();
    saved.sort();
    Ok(saved)
}

/// Every file `dinoco/schema.dinoco` loads, with its path inside a saved copy.
fn current_schema_files() -> anyhow::Result<Vec<(PathBuf, PathBuf)>> {
    let (_, files) = dinoco_compiler::compile_file_with_sources(SCHEMA_PATH).map_err(|err| anyhow!(err.to_string()))?;
    let schema_dir = fs::canonicalize("dinoco").context("dinoco/ was not found")?;
    Ok(files
        .into_iter()
        .map(|file| {
            let relative = snapshot_path(&schema_dir, &file);
            (file, relative)
        })
        .collect())
}

/// The files of `workspace`'s saved schema, relative to the copy's root.
fn saved_schema_files(workspace: &str) -> anyhow::Result<Vec<PathBuf>> {
    fn walk(root: &Path, dir: &Path, files: &mut Vec<PathBuf>) -> anyhow::Result<()> {
        for entry in fs::read_dir(dir).with_context(|| format!("failed to read {}", dir.display()))? {
            let path = entry?.path();
            // Skips files such as .DS_Store that the CLI never saves.
            if path.file_name().and_then(|name| name.to_str()).is_some_and(|name| name.starts_with('.')) {
                continue;
            }
            if path.is_dir() {
                walk(root, &path, files)?;
            } else if let Ok(relative) = path.strip_prefix(root) {
                files.push(relative.to_path_buf());
            }
        }
        Ok(())
    }

    let root = workspace_schema_dir(workspace);
    let mut files = Vec::new();
    walk(&root, &root, &mut files)?;
    Ok(files)
}

/// Whether `workspace`'s saved schema holds exactly the files of `current`.
fn saved_schema_matches(workspace: &str, current: &[(PathBuf, PathBuf)]) -> bool {
    let Ok(mut saved) = saved_schema_files(workspace) else {
        return false;
    };
    let mut expected = current.iter().map(|(_, relative)| relative).collect::<Vec<_>>();
    saved.sort();
    expected.sort();
    let dir = workspace_schema_dir(workspace);
    saved.iter().eq(expected.iter().copied())
        && current.iter().all(|(file, relative)| match (fs::read(file), fs::read(dir.join(relative))) {
            (Ok(current), Ok(saved)) => current == saved,
            _ => false,
        })
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
