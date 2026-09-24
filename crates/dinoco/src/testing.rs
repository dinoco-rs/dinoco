use std::marker::PhantomData;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Context;
use dinoco_compiler::{ConfigValue, Schema};
use dinoco_engine::{
    Backend, DatabaseError, DinocoAdapter, DinocoClient, DinocoEntity, ExecutedQuery, QueryMode, SqliteAdapter,
    TableHooks,
};

/// Creates a client over a fresh in-memory SQLite database with every table,
/// index, and foreign key of `dinoco/schema.dinoco` already created — the
/// same client `connect()` would give you, minus the real database.
///
/// Each call gets its own empty database, so tests can run in parallel. The
/// schema is read relative to the current directory, which is the crate root
/// under `cargo test`. Use [`TestAmbient`] for another path or a workspace.
///
/// ```ignore
/// let client = dinoco::create_test_ambient().await?;
/// dinoco::insert_into::<User>().values(&user).execute(&client).await?;
/// ```
pub async fn create_test_ambient() -> anyhow::Result<DinocoClient> {
    TestAmbient::new().create().await
}

/// Options for [`create_test_ambient`].
#[derive(Debug, Clone)]
pub struct TestAmbient {
    schema_path: PathBuf,
    workspace: Option<String>,
}

impl Default for TestAmbient {
    fn default() -> Self {
        Self { schema_path: PathBuf::from("dinoco/schema.dinoco"), workspace: None }
    }
}

impl TestAmbient {
    pub fn new() -> Self {
        Self::default()
    }

    /// Reads the schema from `path` instead of `dinoco/schema.dinoco`.
    pub fn schema(mut self, path: impl AsRef<Path>) -> Self {
        self.schema_path = path.as_ref().to_path_buf();
        self
    }

    /// Applies the named `config.workspace` entry before building the tables.
    pub fn workspace(mut self, name: impl Into<String>) -> Self {
        self.workspace = Some(name.into());
        self
    }

    pub async fn create(self) -> anyhow::Result<DinocoClient> {
        let schema = self.read_schema()?;
        let adapter = SqliteAdapter::memory().await?;

        // Tables are created in dependency-free order; SQLite checks foreign
        // keys on writes, not when a table references one created later.
        for statement in dinoco_cli::commands::migrate::sqlite_schema_statements(&schema, adapter.clone()) {
            adapter
                .execute(&statement, &[])
                .await
                .with_context(|| format!("failed to create the test schema with `{statement}`"))?;
        }

        Ok(DinocoClient::new(Backend::Sqlite(adapter))
            .with_logger(config_bool(&schema, "with_logger").unwrap_or(false))
            .with_query_mode(match config_string(&schema, "query_mode") {
                Some("single_query") => QueryMode::SingleQuery,
                _ => QueryMode::BatchQuery,
            }))
    }

    fn read_schema(&self) -> anyhow::Result<Schema> {
        let schema = dinoco_compiler::compile_file(&self.schema_path)
            .map_err(|error| anyhow::anyhow!(error.to_string()))
            .with_context(|| format!("failed to compile `{}` for the test ambient", self.schema_path.display()))?;
        let schema = match &self.workspace {
            Some(name) => schema.for_workspace(name).with_context(|| format!("workspace `{name}` was not found"))?,
            None => schema,
        };
        dinoco_cli::schema::validate_schema_relations(&schema)?;

        Ok(schema)
    }
}

fn config_value<'a>(schema: &'a Schema, key: &str) -> Option<&'a ConfigValue> {
    schema.config()?.entries.iter().find(|entry| entry.key == key).map(|entry| &entry.value)
}

fn config_bool(schema: &Schema, key: &str) -> Option<bool> {
    match config_value(schema, key)? {
        ConfigValue::Boolean(value) => Some(*value),
        _ => None,
    }
}

fn config_string<'a>(schema: &'a Schema, key: &str) -> Option<&'a str> {
    match config_value(schema, key)? {
        ConfigValue::String(value) | ConfigValue::Ident(value) => Some(value),
        _ => None,
    }
}

/// Installs callbacks that run after every operation on `M`'s table, whether
/// it runs on `client` directly, inside a `transaction(...)` closure the
/// client opens, or as a relation loaded by `.includes(...)`.
///
/// Each callback receives the result (`None` when the operation failed), the
/// SQL Dinoco compiled for it, and the error (`None` when it succeeded):
///
/// ```ignore
/// dinoco::setup_test_methods::<User>(&client)
///     .on_insert(|inserted, query, error| {
///         println!("{} {:?} {:?}", query.sql, inserted, error.map(|error| error.constraint()));
///     })
///     .on_update(|affected, query, _| println!("{} changed {affected:?} row(s)", query.table));
///
/// dinoco::remove_test_methods::<User>(&client);
/// ```
///
/// Callbacks of other models are left alone. Calling it again for the same
/// model keeps its existing callbacks, and setting one again replaces it.
pub fn setup_test_methods<M>(client: &DinocoClient) -> TestMethods<'_, M>
where
    M: DinocoEntity,
{
    TestMethods { client, marker: PhantomData }
}

/// Removes every callback installed for `M` by [`setup_test_methods`].
pub fn remove_test_methods<M>(client: &DinocoClient)
where
    M: DinocoEntity,
{
    update_table_hooks(client, M::TABLE_NAME, |hooks| *hooks = TableHooks::default());
}

pub struct TestMethods<'a, M> {
    client: &'a DinocoClient,
    marker: PhantomData<fn() -> M>,
}

impl<M> TestMethods<'_, M>
where
    M: DinocoEntity,
{
    /// After each `INSERT` into `M`'s table (including rows inserted as a
    /// nested relation): the inserted rows as JSON objects keyed by column.
    pub fn on_insert<F>(self, callback: F) -> Self
    where
        F: Fn(Option<&[serde_json::Value]>, &ExecutedQuery, Option<&DatabaseError>) + Send + Sync + 'static,
    {
        self.update(|hooks| hooks.on_insert = Some(Arc::new(callback)))
    }

    /// After each `UPDATE` of `M`'s table: how many rows it changed.
    pub fn on_update<F>(self, callback: F) -> Self
    where
        F: Fn(Option<usize>, &ExecutedQuery, Option<&DatabaseError>) + Send + Sync + 'static,
    {
        self.update(|hooks| hooks.on_update = Some(Arc::new(callback)))
    }

    /// After each `DELETE` from `M`'s table: how many rows it removed.
    pub fn on_delete<F>(self, callback: F) -> Self
    where
        F: Fn(Option<usize>, &ExecutedQuery, Option<&DatabaseError>) + Send + Sync + 'static,
    {
        self.update(|hooks| hooks.on_delete = Some(Arc::new(callback)))
    }

    /// After each `find_first`/`find_many` of `M`, and each time `M` is
    /// loaded through `.includes(...)`: how many rows came back.
    pub fn on_find<F>(self, callback: F) -> Self
    where
        F: Fn(Option<usize>, &ExecutedQuery, Option<&DatabaseError>) + Send + Sync + 'static,
    {
        self.update(|hooks| hooks.on_find = Some(Arc::new(callback)))
    }

    fn update(self, update: impl FnOnce(&mut TableHooks)) -> Self {
        update_table_hooks(self.client, M::TABLE_NAME, update);
        self
    }
}

fn update_table_hooks(client: &DinocoClient, table: &'static str, update: impl FnOnce(&mut TableHooks)) {
    let mut hooks = client.query_hooks().map(|hooks| (*hooks).clone()).unwrap_or_default();
    let mut table_hooks = hooks.table(table).cloned().unwrap_or_default();
    update(&mut table_hooks);
    hooks.set_table(table, table_hooks);
    client.set_query_hooks(hooks);
}
