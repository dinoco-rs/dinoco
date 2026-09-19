//! Hand-written ("manual") migrations.
//!
//! A manual migration is a type implementing [`DinocoMigration`]. Its `up` and
//! `down` receive a [`DinocoManager`], which exposes every schema operation the
//! automatic migration engine supports and compiles each one to the dialect of
//! the connected database.

use std::future::Future;
use std::pin::Pin;

use crate::{
    AddColumnMigration, AddForeignKeyMigration, AlterColumnMigration, AlterEnumMigration, Backend, CreateEnumMigration,
    CreateIndexMigration, CreateTableMigration, DeadpoolPostgresRow, DinocoAdapter, DinocoClient, DinocoMysql,
    DinocoPostgres, DinocoSqlCompiler, DinocoSqlite, DropColumnMigration, DropEnumMigration, DropForeignKeyMigration,
    DropIndexMigration, DropTableMigration, MigrationColumn, MigrationColumnType, MigrationDefault, MysqlRow,
    PostgresRow, RenameColumnMigration, RenameTableMigration, SqliteRow,
};

/// The error type returned by manual migrations.
pub type DbErr = anyhow::Error;

/// Table that records which manual migrations were applied. It is separate from
/// the `dinoco_migrations` table used by the automatic engine.
pub const MANUAL_MIGRATIONS_TABLE: &str = "dinoco_manual_migrations";

/// A hand-written migration.
///
/// ```ignore
/// #[dinoco(migration)]
/// pub struct CreateUsers;
///
/// impl DinocoMigration for CreateUsers {
///     async fn up(&self, manager: &DinocoManager) -> Result<(), DbErr> {
///         manager.create_table(/* ... */).await
///     }
///
///     async fn down(&self, manager: &DinocoManager) -> Result<(), DbErr> {
///         manager.drop_table(/* ... */).await
///     }
/// }
/// ```
pub trait DinocoMigration: Send + Sync + 'static {
    fn up(&self, manager: &DinocoManager) -> impl Future<Output = Result<(), DbErr>> + Send;

    fn down(&self, manager: &DinocoManager) -> impl Future<Output = Result<(), DbErr>> + Send;
}

type MigrationFuture<'a> = Pin<Box<dyn Future<Output = Result<(), DbErr>> + Send + 'a>>;

/// Object-safe view of [`DinocoMigration`], implemented automatically.
pub trait DinocoMigrationObject: Send + Sync {
    fn up_boxed<'a>(&'a self, manager: &'a DinocoManager) -> MigrationFuture<'a>;

    fn down_boxed<'a>(&'a self, manager: &'a DinocoManager) -> MigrationFuture<'a>;
}

impl<T: DinocoMigration> DinocoMigrationObject for T {
    fn up_boxed<'a>(&'a self, manager: &'a DinocoManager) -> MigrationFuture<'a> {
        Box::pin(self.up(manager))
    }

    fn down_boxed<'a>(&'a self, manager: &'a DinocoManager) -> MigrationFuture<'a> {
        Box::pin(self.down(manager))
    }
}

/// A named migration in the registry returned by `dinoco/migrations/mod.rs`.
pub struct MigrationEntry {
    pub name: &'static str,
    pub migration: Box<dyn DinocoMigrationObject>,
}

impl MigrationEntry {
    pub fn new<M: DinocoMigration>(name: &'static str, migration: M) -> Self {
        Self { name, migration: Box::new(migration) }
    }
}

/// Runs schema operations for a manual migration.
pub struct DinocoManager {
    backend: Backend,
}

macro_rules! with_adapter {
    ($backend:expr, $adapter:ident => $body:expr) => {
        match $backend {
            Backend::Sqlite($adapter) => $body,
            Backend::Postgres($adapter) => $body,
            Backend::PgBouncer($adapter) => $body,
            Backend::Mysql($adapter) => $body,
        }
    };
}

impl DinocoManager {
    /// `backend` is cloned by the caller; clones share the connection pool.
    pub fn new(backend: Backend) -> Self {
        Self { backend }
    }

    /// The connection the migration runs against, for anything the manager has
    /// no dedicated method for.
    pub fn backend(&self) -> &Backend {
        &self.backend
    }

    /// Executes one raw SQL statement.
    pub async fn execute(&self, sql: &str) -> Result<(), DbErr> {
        with_adapter!(&self.backend, adapter => adapter.execute(sql, &[]).await.map(|_| ()))
    }

    /// Runs compiled statements. A dialect that cannot perform an operation
    /// compiles it to a SQL comment explaining why; executing that would
    /// silently do nothing, so it is reported as an error instead.
    async fn execute_all(&self, statements: Vec<String>) -> Result<(), DbErr> {
        for statement in statements {
            if statement.lines().all(|line| line.trim().is_empty() || line.trim_start().starts_with("--")) {
                anyhow::bail!(
                    "this operation is not supported by the connected database: {}",
                    statement.trim().trim_start_matches("--").trim()
                );
            }
            self.execute(&statement).await?;
        }
        Ok(())
    }

    pub async fn create_table(&self, migration: CreateTableMigration) -> Result<(), DbErr> {
        let sql = with_adapter!(&self.backend, adapter => adapter.compile_create_table_migration(migration));
        self.execute(&sql).await
    }

    pub async fn drop_table(&self, migration: DropTableMigration) -> Result<(), DbErr> {
        let sql = with_adapter!(&self.backend, adapter => adapter.compile_drop_table_migration(migration));
        self.execute(&sql).await
    }

    pub async fn rename_table(&self, migration: RenameTableMigration) -> Result<(), DbErr> {
        let sql = with_adapter!(&self.backend, adapter => adapter.compile_rename_table_migration(migration));
        self.execute_all(sql).await
    }

    pub async fn add_column(&self, migration: AddColumnMigration) -> Result<(), DbErr> {
        let sql = with_adapter!(&self.backend, adapter => adapter.compile_add_column_migration(migration));
        self.execute(&sql).await
    }

    pub async fn drop_column(&self, migration: DropColumnMigration) -> Result<(), DbErr> {
        let sql = with_adapter!(&self.backend, adapter => adapter.compile_drop_column_migration(migration));
        self.execute(&sql).await
    }

    pub async fn alter_column(&self, migration: AlterColumnMigration) -> Result<(), DbErr> {
        let sql = with_adapter!(&self.backend, adapter => adapter.compile_alter_column_migration(migration));
        self.execute_all(sql).await
    }

    pub async fn rename_column(&self, migration: RenameColumnMigration) -> Result<(), DbErr> {
        let sql = with_adapter!(&self.backend, adapter => adapter.compile_rename_column_migration(migration));
        self.execute_all(sql).await
    }

    pub async fn add_foreign_key(&self, migration: AddForeignKeyMigration) -> Result<(), DbErr> {
        let sql = with_adapter!(&self.backend, adapter => adapter.compile_add_foreign_key_migration(migration));
        self.execute_all(sql).await
    }

    pub async fn drop_foreign_key(&self, migration: DropForeignKeyMigration) -> Result<(), DbErr> {
        let sql = with_adapter!(&self.backend, adapter => adapter.compile_drop_foreign_key_migration(migration));
        self.execute_all(sql).await
    }

    pub async fn create_index(&self, migration: CreateIndexMigration) -> Result<(), DbErr> {
        let sql = with_adapter!(&self.backend, adapter => adapter.compile_create_index_migration(migration));
        self.execute(&sql).await
    }

    pub async fn drop_index(&self, migration: DropIndexMigration) -> Result<(), DbErr> {
        let sql = with_adapter!(&self.backend, adapter => adapter.compile_drop_index_migration(migration));
        self.execute(&sql).await
    }

    pub async fn create_enum(&self, migration: CreateEnumMigration) -> Result<(), DbErr> {
        let sql = with_adapter!(&self.backend, adapter => adapter.compile_create_enum_migration(migration));
        self.execute_all(sql).await
    }

    pub async fn drop_enum(&self, migration: DropEnumMigration) -> Result<(), DbErr> {
        let sql = with_adapter!(&self.backend, adapter => adapter.compile_drop_enum_migration(migration));
        self.execute_all(sql).await
    }

    pub async fn alter_enum(&self, migration: AlterEnumMigration) -> Result<(), DbErr> {
        let sql = with_adapter!(&self.backend, adapter => adapter.compile_alter_enum_migration(migration));
        self.execute_all(sql).await
    }
}

impl MigrationColumn {
    /// A required, non-unique column without a default.
    pub fn new(name: impl Into<String>, ty: MigrationColumnType) -> Self {
        Self { name: name.into(), ty, primary_key: false, unique: false, nullable: false, default: None }
    }

    pub fn primary_key(mut self) -> Self {
        self.primary_key = true;
        self
    }

    pub fn unique(mut self) -> Self {
        self.unique = true;
        self
    }

    pub fn nullable(mut self) -> Self {
        self.nullable = true;
        self
    }

    pub fn default(mut self, default: MigrationDefault) -> Self {
        self.default = Some(default);
        self
    }
}

/// The state of one migration, as reported by [`migration_status`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationStatus {
    pub name: String,
    pub applied: bool,
}

struct AppliedRow {
    name: String,
}

impl DinocoSqlite for AppliedRow {
    fn from_sqlite_row(row: &SqliteRow<'_>) -> Option<Self> {
        row.get::<_, String>("name").ok().map(|name| Self { name })
    }
}

impl DinocoPostgres for AppliedRow {
    fn from_deadpool_posgres_row(row: &DeadpoolPostgresRow) -> Option<Self> {
        row.try_get::<_, String>("name").ok().map(|name| Self { name })
    }

    fn from_postgres_row(row: &PostgresRow) -> Option<Self> {
        Self::from_deadpool_posgres_row(row)
    }
}

impl DinocoMysql for AppliedRow {
    fn from_mysql_row(row: &MysqlRow) -> Option<Self> {
        row.get::<String, _>("name").map(|name| Self { name })
    }
}

fn quote_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

async fn ensure_history_table(backend: &Backend) -> Result<(), DbErr> {
    let sql = format!(
        "CREATE TABLE IF NOT EXISTS {MANUAL_MIGRATIONS_TABLE} (name VARCHAR(255) PRIMARY KEY NOT NULL, applied_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP)"
    );
    DinocoManager::new(backend.clone()).execute(&sql).await
}

async fn applied_names(backend: &Backend) -> Result<Vec<String>, DbErr> {
    let sql = format!("SELECT name FROM {MANUAL_MIGRATIONS_TABLE} ORDER BY name");
    let rows: Vec<AppliedRow> = with_adapter!(backend, adapter => adapter.query::<AppliedRow>(&sql, &[]).await)?;
    Ok(rows.into_iter().map(|row| row.name).collect())
}

fn validate_registry(entries: &[MigrationEntry]) -> Result<(), DbErr> {
    let mut seen = std::collections::BTreeSet::new();
    for entry in entries {
        if !seen.insert(entry.name) {
            anyhow::bail!("manual migration `{}` is registered more than once", entry.name);
        }
    }
    Ok(())
}

/// Lists every registered migration and whether it was applied. Applied
/// migrations that are no longer registered are an error: the history would no
/// longer match the code.
pub async fn migration_status(
    client: &DinocoClient,
    entries: &[MigrationEntry],
) -> Result<Vec<MigrationStatus>, DbErr> {
    validate_registry(entries)?;
    ensure_history_table(&client.backend).await?;
    let applied = applied_names(&client.backend).await?;

    if let Some(unknown) = applied.iter().find(|name| !entries.iter().any(|entry| entry.name == name.as_str())) {
        anyhow::bail!("migration `{unknown}` was applied but is not registered in dinoco/migrations/mod.rs");
    }

    Ok(entries
        .iter()
        .map(|entry| MigrationStatus {
            name: entry.name.to_string(),
            applied: applied.iter().any(|name| name == entry.name),
        })
        .collect())
}

/// Applies every pending migration, oldest first, and returns the names that
/// ran. Stops at the first failure without recording it.
pub async fn migrate_up(client: &DinocoClient, entries: &[MigrationEntry]) -> Result<Vec<String>, DbErr> {
    let status = migration_status(client, entries).await?;
    let manager = DinocoManager::new(client.backend.clone());
    let mut ran = Vec::new();

    for (entry, state) in entries.iter().zip(status) {
        if state.applied {
            continue;
        }

        entry
            .migration
            .up_boxed(&manager)
            .await
            .map_err(|error| error.context(format!("migration `{}` failed while applying `up`", entry.name)))?;
        manager
            .execute(&format!("INSERT INTO {MANUAL_MIGRATIONS_TABLE} (name) VALUES ({})", quote_literal(entry.name)))
            .await?;
        ran.push(entry.name.to_string());
    }

    Ok(ran)
}

/// Reverts the last `steps` applied migrations, newest first, and returns the
/// names that were reverted.
pub async fn migrate_down(
    client: &DinocoClient,
    entries: &[MigrationEntry],
    steps: usize,
) -> Result<Vec<String>, DbErr> {
    let status = migration_status(client, entries).await?;
    let manager = DinocoManager::new(client.backend.clone());
    let mut reverted = Vec::new();

    for (entry, state) in entries.iter().zip(status).rev() {
        if reverted.len() >= steps {
            break;
        }
        if !state.applied {
            continue;
        }

        entry
            .migration
            .down_boxed(&manager)
            .await
            .map_err(|error| error.context(format!("migration `{}` failed while applying `down`", entry.name)))?;
        manager
            .execute(&format!("DELETE FROM {MANUAL_MIGRATIONS_TABLE} WHERE name = {}", quote_literal(entry.name)))
            .await?;
        reverted.push(entry.name.to_string());
    }

    Ok(reverted)
}
