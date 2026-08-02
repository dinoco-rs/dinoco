use std::collections::BTreeMap;

use anyhow::Context;
use dinoco_engine::{
    AddColumnMigration, AddForeignKeyMigration, AlterColumnMigration, AlterEnumMigration, CreateEnumMigration,
    CreateIndexMigration, CreateTableMigration, DinocoAdapter, DinocoSqlCompiler, DropColumnMigration,
    DropEnumMigration, DropForeignKeyMigration, DropIndexMigration, DropTableMigration, MigrationColumn,
    MigrationColumnType, MigrationForeignKey, MigrationIndex, MigrationIndexKind, MySqlAdapter, PgBouncerAdapter,
    PostgresAdapter, ReferentialAction, RenameColumnMigration, SqliteAdapter,
};
use dinoco_engine::{
    mysql_async::{TxOpts, prelude::Queryable},
    rusqlite,
};
use rusqlite::{
    OptionalExtension,
    hooks::{AuthAction, AuthContext, Authorization},
};

use crate::schema::{Database, PostgresConnection, RuntimeConfig};

pub enum CliDatabase {
    Postgres(PostgresAdapter),
    PgBouncer(PgBouncerAdapter),
    Mysql(MySqlAdapter),
    Sqlite(SqliteAdapter),
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct DatabaseSchema {
    pub tables: Vec<DatabaseTable>,
    pub enums: Vec<DatabaseEnum>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DatabaseTable {
    pub name: String,
    pub row_count: i64,
    pub columns: Vec<MigrationColumn>,
    pub foreign_keys: Vec<MigrationForeignKey>,
    #[serde(default)]
    pub indexes: Vec<MigrationIndex>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DatabaseEnum {
    pub name: String,
    pub values: Vec<String>,
}

pub struct MigrationMetadata {
    pub history_exists: bool,
    pub applied: Vec<String>,
    pub checksums: Option<BTreeMap<String, String>>,
    pub checksums_required: bool,
    pub schema_snapshots: Option<BTreeMap<String, String>>,
    pub schema_snapshots_required: bool,
}

pub type SqliteMigrationMetadata = MigrationMetadata;

const POSTGRES_MIGRATION_LOCK_ID: i64 = 0x4449_4E4F_434F;
const MYSQL_MIGRATION_LOCK_SQL: &str = "SELECT GET_LOCK(CONCAT('dinoco:migrations:', DATABASE()), 30)";
const MYSQL_MIGRATION_UNLOCK_SQL: &str = "SELECT RELEASE_LOCK(CONCAT('dinoco:migrations:', DATABASE()))";
const POSTGRES_CHECKSUMS_TABLE_SQL: &str = "CREATE TABLE IF NOT EXISTS dinoco_migration_checksums (name VARCHAR(255) PRIMARY KEY, checksum VARCHAR(64) NOT NULL)";
const POSTGRES_CHECKSUM_GUARD_SQL: &str =
    "CREATE INDEX IF NOT EXISTS dinoco_migrations_checksum_required ON dinoco_migrations(name)";
const MYSQL_CHECKSUMS_TABLE_SQL: &str = "CREATE TABLE IF NOT EXISTS dinoco_migration_checksums (name VARCHAR(255) PRIMARY KEY, checksum VARCHAR(64) NOT NULL)";
const POSTGRES_SCHEMA_SNAPSHOTS_TABLE_SQL: &str =
    "CREATE TABLE IF NOT EXISTS dinoco_migration_schemas (name VARCHAR(255) PRIMARY KEY, schema_json TEXT NOT NULL)";
const POSTGRES_SCHEMA_SNAPSHOT_GUARD_SQL: &str =
    "CREATE INDEX IF NOT EXISTS dinoco_migrations_schema_snapshots_required ON dinoco_migrations(name)";
const MYSQL_SCHEMA_SNAPSHOTS_TABLE_SQL: &str = "CREATE TABLE IF NOT EXISTS dinoco_migration_schemas (name VARCHAR(255) PRIMARY KEY, schema_json LONGTEXT NOT NULL)";

impl CliDatabase {
    pub async fn connect(config: &RuntimeConfig) -> anyhow::Result<Self> {
        match (config.database, config.postgres_connection) {
            (Database::Postgresql, PostgresConnection::Direct) => Ok(Self::Postgres(
                PostgresAdapter::direct_with_pool(&config.database_url, config.min_connection, config.max_connection)
                    .await?,
            )),
            (Database::Postgresql, PostgresConnection::PgBouncer) => {
                Ok(Self::PgBouncer(PgBouncerAdapter::new(&config.database_url).await?))
            }
            (Database::Mysql, _) => Ok(Self::Mysql(MySqlAdapter::new(&config.database_url))),
            (Database::Sqlite, _) => {
                let adapter = SqliteAdapter::new(config.database_url.clone()).await.map_err(anyhow::Error::msg)?;
                Ok(Self::Sqlite(adapter))
            }
        }
    }

    pub async fn execute(&self, sql: &str) -> anyhow::Result<usize> {
        match self {
            Self::Postgres(adapter) => adapter.execute(sql, &[]).await,
            Self::PgBouncer(adapter) => adapter.execute(sql, &[]).await,
            Self::Mysql(adapter) => adapter.execute(sql, &[]).await,
            Self::Sqlite(adapter) => adapter.execute(sql, &[]).await,
        }
    }

    pub fn is_sqlite(&self) -> bool {
        matches!(self, Self::Sqlite(_))
    }

    pub async fn execute_transaction(&self, statements: &[String]) -> anyhow::Result<()> {
        match self {
            Self::Sqlite(adapter) => {
                let conn = adapter.pool.get().await.context("failed to get sqlite connection from pool")?;
                let statements = statements.to_vec();

                conn.interact(move |conn| -> anyhow::Result<()> {
                    let transaction = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
                    transaction.pragma_update(None, "defer_foreign_keys", true)?;
                    ensure_sqlite_migration_checksum_guard(&transaction)?;
                    transaction.authorizer(Some(sqlite_migration_authorizer))?;
                    let execution = statements.iter().try_for_each(|statement| transaction.execute_batch(statement));
                    transaction.authorizer(None::<fn(AuthContext<'_>) -> Authorization>)?;
                    execution?;
                    ensure_sqlite_foreign_key_integrity(&transaction)?;
                    transaction.commit()?;
                    Ok(())
                })
                .await
                .map_err(|err| anyhow::anyhow!(err.to_string()))?
            }
            Self::Postgres(adapter) => execute_postgres_transaction(adapter, statements).await,
            Self::PgBouncer(adapter) => execute_postgres_transaction(adapter.inner(), statements).await,
            Self::Mysql(adapter) => execute_mysql_transaction(adapter, statements).await,
        }
    }

    pub async fn apply_server_migration(
        &self,
        name: &str,
        statements: &[String],
        checksum: &str,
    ) -> anyhow::Result<bool> {
        match self {
            Self::Postgres(adapter) => apply_postgres_migration(adapter, name, statements, checksum).await,
            Self::PgBouncer(adapter) => apply_postgres_migration(adapter.inner(), name, statements, checksum).await,
            Self::Mysql(adapter) => apply_mysql_migration(adapter, name, statements, checksum).await,
            Self::Sqlite(_) => anyhow::bail!("server migration application is not available for SQLite databases"),
        }
    }

    pub async fn record_legacy_migration_checksums(&self, checksums: &[(String, String)]) -> anyhow::Result<()> {
        if checksums.is_empty() {
            return Ok(());
        }

        match self {
            Self::Postgres(adapter) => record_postgres_legacy_checksums(adapter, checksums).await,
            Self::PgBouncer(adapter) => record_postgres_legacy_checksums(adapter.inner(), checksums).await,
            Self::Mysql(adapter) => record_mysql_legacy_checksums(adapter, checksums).await,
            Self::Sqlite(_) => anyhow::bail!("use SQLite checksum statements for SQLite migration history"),
        }
    }

    pub async fn record_server_schema_snapshot(&self, name: &str, schema: &DatabaseSchema) -> anyhow::Result<()> {
        let schema_json = serde_json::to_string(schema).context("failed to serialize migration schema snapshot")?;
        match self {
            Self::Postgres(adapter) => record_postgres_schema_snapshot(adapter, name, &schema_json).await,
            Self::PgBouncer(adapter) => record_postgres_schema_snapshot(adapter.inner(), name, &schema_json).await,
            Self::Mysql(adapter) => record_mysql_schema_snapshot(adapter, name, &schema_json).await,
            Self::Sqlite(_) => anyhow::bail!("SQLite reconstructs schema snapshots from migration history"),
        }
    }

    pub async fn replay_sqlite_history_migration(&self, sql: String, generated: bool) -> anyhow::Result<()> {
        let Self::Sqlite(adapter) = self else {
            anyhow::bail!("SQLite history replay is only available for SQLite databases");
        };
        let conn = adapter.pool.get().await.context("failed to get sqlite history connection from pool")?;

        conn.interact(move |conn| -> anyhow::Result<()> {
            let transaction = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
            transaction.pragma_update(None, "defer_foreign_keys", true)?;
            if generated {
                transaction.authorizer(Some(sqlite_migration_authorizer))?;
            } else {
                transaction.authorizer(Some(sqlite_custom_migration_authorizer))?;
            }
            let replay = transaction.execute_batch(&sql);
            transaction.authorizer(None::<fn(AuthContext<'_>) -> Authorization>)?;
            replay?;
            ensure_sqlite_foreign_key_integrity(&transaction)?;
            transaction.commit()?;
            Ok(())
        })
        .await
        .map_err(|err| anyhow::anyhow!(err.to_string()))?
    }

    pub async fn apply_sqlite_migration(
        &self,
        name: String,
        sql: String,
        checksum: String,
        generated: bool,
    ) -> anyhow::Result<bool> {
        let Self::Sqlite(adapter) = self else {
            anyhow::bail!("atomic SQLite migration application is only available for SQLite databases");
        };
        let conn = adapter.pool.get().await.context("failed to get sqlite migration connection from pool")?;

        conn.interact(move |conn| -> anyhow::Result<bool> {
            let transaction = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
            transaction.pragma_update(None, "defer_foreign_keys", true)?;
            transaction.execute_batch(
                "CREATE TABLE IF NOT EXISTS dinoco_migrations (
                        name TEXT PRIMARY KEY,
                        applied_at TEXT DEFAULT CURRENT_TIMESTAMP
                    );
                    CREATE TABLE IF NOT EXISTS dinoco_migration_checksums (
                        name TEXT PRIMARY KEY,
                        checksum TEXT NOT NULL
                    );",
            )?;
            ensure_sqlite_migration_checksum_guard(&transaction)?;
            let already_applied: bool = transaction.query_row(
                "SELECT EXISTS(SELECT 1 FROM dinoco_migrations WHERE name = ?1)",
                [&name],
                |row| row.get(0),
            )?;
            if already_applied {
                let recorded_checksum = transaction
                    .query_row(
                        "SELECT checksum FROM dinoco_migration_checksums WHERE name = ?1",
                        [&name],
                        |row| row.get::<_, String>(0),
                    )
                    .optional()?;
                match recorded_checksum {
                    Some(recorded) if recorded == checksum => {}
                    Some(recorded) => {
                        anyhow::bail!(
                            "Migration `{name}` was applied concurrently with checksum {recorded}, but this runner validated checksum {checksum}. Refusing to continue with divergent migration history."
                        );
                    }
                    None => {
                        anyhow::bail!(
                            "Migration `{name}` was applied concurrently without a checksum record. Refusing to continue with unverifiable migration history."
                        );
                    }
                }
                ensure_sqlite_foreign_key_integrity(&transaction)?;
                transaction.commit()?;
                return Ok(false);
            }

            if generated {
                transaction.authorizer(Some(sqlite_migration_authorizer))?;
            } else {
                transaction.authorizer(Some(sqlite_custom_migration_authorizer))?;
            }
            let application = transaction.execute_batch(&sql);
            transaction.authorizer(None::<fn(AuthContext<'_>) -> Authorization>)?;
            application?;
            transaction.execute("INSERT OR IGNORE INTO dinoco_migrations (name) VALUES (?1)", [&name])?;
            transaction.execute(
                "INSERT INTO dinoco_migration_checksums (name, checksum) VALUES (?1, ?2)
                 ON CONFLICT(name) DO UPDATE SET checksum =
                     CASE
                         WHEN dinoco_migration_checksums.checksum = excluded.checksum
                         THEN dinoco_migration_checksums.checksum
                         ELSE NULL
                     END",
                rusqlite::params![&name, &checksum],
            )?;
            ensure_sqlite_foreign_key_integrity(&transaction)?;
            transaction.commit()?;
            Ok(true)
        })
        .await
        .map_err(|err| anyhow::anyhow!(err.to_string()))?
    }

    pub async fn count(&self, sql: &str) -> anyhow::Result<i64> {
        match self {
            Self::Postgres(adapter) => adapter.query_count(sql, &[]).await,
            Self::PgBouncer(adapter) => adapter.query_count(sql, &[]).await,
            Self::Mysql(adapter) => adapter.query_count(sql, &[]).await,
            Self::Sqlite(adapter) => adapter.query_count(sql, &[]).await,
        }
    }

    pub async fn inspect_schema(&self) -> anyhow::Result<DatabaseSchema> {
        match self {
            Self::Sqlite(adapter) => inspect_sqlite(adapter).await,
            Self::Postgres(adapter) => inspect_postgres(adapter).await,
            Self::PgBouncer(adapter) => inspect_pgbouncer(adapter).await,
            Self::Mysql(adapter) => inspect_mysql(adapter).await,
        }
    }

    pub async fn migrations_table_exists(&self) -> anyhow::Result<bool> {
        let sql = match self {
            Self::Postgres(_) | Self::PgBouncer(_) => {
                "SELECT COUNT(*) FROM information_schema.tables WHERE table_schema = 'public' AND table_name = 'dinoco_migrations'"
            }
            Self::Mysql(_) => {
                "SELECT COUNT(*) FROM information_schema.tables WHERE table_schema = DATABASE() AND table_name = 'dinoco_migrations'"
            }
            Self::Sqlite(_) => "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'dinoco_migrations'",
        };

        Ok(self.count(sql).await? > 0)
    }

    pub async fn database_has_user_tables(&self) -> anyhow::Result<bool> {
        let sql = match self {
            Self::Postgres(_) | Self::PgBouncer(_) => {
                "SELECT COUNT(*) FROM information_schema.tables WHERE table_schema = 'public' AND table_type = 'BASE TABLE' AND table_name NOT IN ('dinoco_migrations', 'dinoco_migration_checksums', 'dinoco_migration_schemas')"
            }
            Self::Mysql(_) => {
                "SELECT COUNT(*) FROM information_schema.tables WHERE table_schema = DATABASE() AND table_type = 'BASE TABLE' AND table_name NOT IN ('dinoco_migrations', 'dinoco_migration_checksums', 'dinoco_migration_schemas')"
            }
            Self::Sqlite(_) => {
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' AND name NOT IN ('dinoco_migrations', 'dinoco_migration_checksums', 'dinoco_migration_schemas')"
            }
        };

        Ok(self.count(sql).await.context("failed to inspect database tables")? > 0)
    }

    pub async fn migration_applied(&self, name: &str) -> anyhow::Result<bool> {
        let name = name.replace('\'', "''");
        let sql = format!("SELECT COUNT(*) FROM dinoco_migrations WHERE name = '{name}'");

        Ok(self.count(&sql).await? > 0)
    }

    pub async fn migration_metadata(&self) -> anyhow::Result<MigrationMetadata> {
        match self {
            Self::Postgres(adapter) => postgres_migration_metadata(adapter).await,
            Self::PgBouncer(adapter) => postgres_migration_metadata(adapter.inner()).await,
            Self::Mysql(adapter) => mysql_migration_metadata(adapter).await,
            Self::Sqlite(_) => self.sqlite_migration_metadata().await,
        }
    }

    pub async fn sqlite_migration_metadata(&self) -> anyhow::Result<SqliteMigrationMetadata> {
        let Self::Sqlite(adapter) = self else {
            anyhow::bail!("SQLite migration metadata is only available for SQLite databases");
        };
        let conn = adapter.pool.get().await.context("failed to get sqlite metadata connection from pool")?;
        conn.interact(move |conn| -> anyhow::Result<SqliteMigrationMetadata> {
            let transaction = conn.transaction()?;
            let migrations_table_exists: bool = transaction.query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'dinoco_migrations')",
                [],
                |row| row.get(0),
            )?;
            let applied = if migrations_table_exists {
                let mut statement = transaction.prepare("SELECT name FROM dinoco_migrations ORDER BY name")?;
                statement.query_map([], |row| row.get::<_, String>(0))?.collect::<Result<Vec<_>, _>>()?
            } else {
                Vec::new()
            };

            let checksums_table_exists: bool = transaction.query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'dinoco_migration_checksums')",
                [],
                |row| row.get(0),
            )?;
            let checksums = if checksums_table_exists {
                let mut statement =
                    transaction.prepare("SELECT name, checksum FROM dinoco_migration_checksums ORDER BY name")?;
                Some(
                    statement
                        .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))?
                        .collect::<Result<BTreeMap<_, _>, _>>()?,
                )
            } else {
                None
            };
            let checksums_required: bool = transaction.query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'index' AND name = 'dinoco_migrations_checksum_required')",
                [],
                |row| row.get(0),
            )?;
            transaction.commit()?;
            Ok(SqliteMigrationMetadata {
                history_exists: migrations_table_exists,
                applied,
                checksums,
                checksums_required,
                schema_snapshots: None,
                schema_snapshots_required: false,
            })
        })
        .await
        .map_err(|err| anyhow::anyhow!(err.to_string()))?
    }

    pub fn compile_create_migration_checksums_table(&self) -> String {
        debug_assert!(self.is_sqlite());
        "CREATE TABLE IF NOT EXISTS dinoco_migration_checksums (name TEXT PRIMARY KEY, checksum TEXT NOT NULL)"
            .to_string()
    }

    pub fn compile_create_migration_checksum_guard(&self) -> String {
        debug_assert!(self.is_sqlite());
        "CREATE INDEX IF NOT EXISTS dinoco_migrations_checksum_required ON dinoco_migrations(name)".to_string()
    }

    pub fn compile_insert_migration_checksum(&self, name: &str, checksum: &str) -> String {
        debug_assert!(self.is_sqlite());
        format!(
            "INSERT INTO dinoco_migration_checksums (name, checksum) VALUES ('{}', '{}') \
             ON CONFLICT(name) DO UPDATE SET checksum = CASE \
             WHEN dinoco_migration_checksums.checksum = excluded.checksum \
             THEN dinoco_migration_checksums.checksum ELSE NULL END",
            escape_sql_literal(name),
            escape_sql_literal(checksum)
        )
    }

    pub fn compile_insert_migration_checksum_if_applied(&self, name: &str, checksum: &str) -> String {
        debug_assert!(self.is_sqlite());
        format!(
            "INSERT INTO dinoco_migration_checksums (name, checksum) \
             SELECT '{}', '{}' WHERE EXISTS (SELECT 1 FROM dinoco_migrations WHERE name = '{}') \
             ON CONFLICT(name) DO UPDATE SET checksum = CASE \
             WHEN dinoco_migration_checksums.checksum = excluded.checksum \
             THEN dinoco_migration_checksums.checksum ELSE NULL END",
            escape_sql_literal(name),
            escape_sql_literal(checksum),
            escape_sql_literal(name)
        )
    }

    pub fn compile_create_migrations_table(&self) -> String {
        match self {
            Self::Postgres(adapter) => adapter.compile_create_migrations_table(),
            Self::PgBouncer(adapter) => adapter.compile_create_migrations_table(),
            Self::Mysql(adapter) => adapter.compile_create_migrations_table(),
            Self::Sqlite(adapter) => adapter.compile_create_migrations_table(),
        }
    }

    pub fn compile_insert_migration_record(&self, name: &str) -> String {
        match self {
            Self::Postgres(adapter) => adapter.compile_insert_migration_record(name),
            Self::PgBouncer(adapter) => adapter.compile_insert_migration_record(name),
            Self::Mysql(adapter) => adapter.compile_insert_migration_record(name),
            Self::Sqlite(adapter) => adapter.compile_insert_migration_record(name),
        }
    }

    pub fn compile_create_table_migration(&self, migration: CreateTableMigration) -> String {
        match self {
            Self::Postgres(adapter) => adapter.compile_create_table_migration(migration),
            Self::PgBouncer(adapter) => adapter.compile_create_table_migration(migration),
            Self::Mysql(adapter) => adapter.compile_create_table_migration(migration),
            Self::Sqlite(adapter) => adapter.compile_create_table_migration(migration),
        }
    }

    pub fn compile_drop_table_migration(&self, migration: DropTableMigration) -> String {
        match self {
            Self::Postgres(adapter) => adapter.compile_drop_table_migration(migration),
            Self::PgBouncer(adapter) => adapter.compile_drop_table_migration(migration),
            Self::Mysql(adapter) => adapter.compile_drop_table_migration(migration),
            Self::Sqlite(adapter) => adapter.compile_drop_table_migration(migration),
        }
    }

    pub fn compile_add_column_migration(&self, migration: AddColumnMigration) -> String {
        match self {
            Self::Postgres(adapter) => adapter.compile_add_column_migration(migration),
            Self::PgBouncer(adapter) => adapter.compile_add_column_migration(migration),
            Self::Mysql(adapter) => adapter.compile_add_column_migration(migration),
            Self::Sqlite(adapter) => adapter.compile_add_column_migration(migration),
        }
    }

    pub fn compile_drop_column_migration(&self, migration: DropColumnMigration) -> String {
        match self {
            Self::Postgres(adapter) => adapter.compile_drop_column_migration(migration),
            Self::PgBouncer(adapter) => adapter.compile_drop_column_migration(migration),
            Self::Mysql(adapter) => adapter.compile_drop_column_migration(migration),
            Self::Sqlite(adapter) => adapter.compile_drop_column_migration(migration),
        }
    }

    pub fn compile_alter_column_migration(&self, migration: AlterColumnMigration) -> Vec<String> {
        match self {
            Self::Postgres(adapter) => adapter.compile_alter_column_migration(migration),
            Self::PgBouncer(adapter) => adapter.compile_alter_column_migration(migration),
            Self::Mysql(adapter) => adapter.compile_alter_column_migration(migration),
            Self::Sqlite(adapter) => adapter.compile_alter_column_migration(migration),
        }
    }

    pub fn compile_rename_column_migration(&self, migration: RenameColumnMigration) -> Vec<String> {
        match self {
            Self::Postgres(adapter) => adapter.compile_rename_column_migration(migration),
            Self::PgBouncer(adapter) => adapter.compile_rename_column_migration(migration),
            Self::Mysql(adapter) => adapter.compile_rename_column_migration(migration),
            Self::Sqlite(adapter) => adapter.compile_rename_column_migration(migration),
        }
    }

    pub fn compile_add_foreign_key_migration(&self, migration: AddForeignKeyMigration) -> Vec<String> {
        match self {
            Self::Postgres(adapter) => adapter.compile_add_foreign_key_migration(migration),
            Self::PgBouncer(adapter) => adapter.compile_add_foreign_key_migration(migration),
            Self::Mysql(adapter) => adapter.compile_add_foreign_key_migration(migration),
            Self::Sqlite(adapter) => adapter.compile_add_foreign_key_migration(migration),
        }
    }

    pub fn compile_drop_foreign_key_migration(&self, migration: DropForeignKeyMigration) -> Vec<String> {
        match self {
            Self::Postgres(adapter) => adapter.compile_drop_foreign_key_migration(migration),
            Self::PgBouncer(adapter) => adapter.compile_drop_foreign_key_migration(migration),
            Self::Mysql(adapter) => adapter.compile_drop_foreign_key_migration(migration),
            Self::Sqlite(adapter) => adapter.compile_drop_foreign_key_migration(migration),
        }
    }

    pub fn compile_create_index_migration(&self, migration: CreateIndexMigration) -> String {
        match self {
            Self::Postgres(adapter) => adapter.compile_create_index_migration(migration),
            Self::PgBouncer(adapter) => adapter.compile_create_index_migration(migration),
            Self::Mysql(adapter) => adapter.compile_create_index_migration(migration),
            Self::Sqlite(adapter) => adapter.compile_create_index_migration(migration),
        }
    }

    pub fn compile_drop_index_migration(&self, migration: DropIndexMigration) -> String {
        match self {
            Self::Postgres(adapter) => adapter.compile_drop_index_migration(migration),
            Self::PgBouncer(adapter) => adapter.compile_drop_index_migration(migration),
            Self::Mysql(adapter) => adapter.compile_drop_index_migration(migration),
            Self::Sqlite(adapter) => adapter.compile_drop_index_migration(migration),
        }
    }

    pub fn compile_create_enum_migration(&self, migration: CreateEnumMigration) -> Vec<String> {
        match self {
            Self::Postgres(adapter) => adapter.compile_create_enum_migration(migration),
            Self::PgBouncer(adapter) => adapter.compile_create_enum_migration(migration),
            Self::Mysql(adapter) => adapter.compile_create_enum_migration(migration),
            Self::Sqlite(adapter) => adapter.compile_create_enum_migration(migration),
        }
    }

    pub fn compile_drop_enum_migration(&self, migration: DropEnumMigration) -> Vec<String> {
        match self {
            Self::Postgres(adapter) => adapter.compile_drop_enum_migration(migration),
            Self::PgBouncer(adapter) => adapter.compile_drop_enum_migration(migration),
            Self::Mysql(adapter) => adapter.compile_drop_enum_migration(migration),
            Self::Sqlite(adapter) => adapter.compile_drop_enum_migration(migration),
        }
    }

    pub fn compile_alter_enum_migration(&self, migration: AlterEnumMigration) -> Vec<String> {
        match self {
            Self::Postgres(adapter) => adapter.compile_alter_enum_migration(migration),
            Self::PgBouncer(adapter) => adapter.compile_alter_enum_migration(migration),
            Self::Mysql(adapter) => adapter.compile_alter_enum_migration(migration),
            Self::Sqlite(adapter) => adapter.compile_alter_enum_migration(migration),
        }
    }
}

async fn execute_postgres_transaction(adapter: &PostgresAdapter, statements: &[String]) -> anyhow::Result<()> {
    let mut conn = adapter.pool.get().await.context("failed to get postgres migration connection from pool")?;
    let transaction = conn.transaction().await?;
    transaction
        .query_one("SELECT pg_advisory_xact_lock($1)", &[&POSTGRES_MIGRATION_LOCK_ID])
        .await
        .context("failed to acquire the PostgreSQL migration lock")?;
    for statement in statements {
        transaction.batch_execute(statement).await?;
    }
    transaction.commit().await?;
    Ok(())
}

async fn execute_mysql_transaction(adapter: &MySqlAdapter, statements: &[String]) -> anyhow::Result<()> {
    let mut conn = adapter.pool.get_conn().await.context("failed to get mysql migration connection from pool")?;
    acquire_mysql_migration_lock(&mut conn).await?;
    let execution = async {
        conn.query_drop("START TRANSACTION").await?;
        for statement in statements {
            if let Err(error) = conn.query_drop(statement).await {
                let _ = conn.query_drop("ROLLBACK").await;
                return Err(anyhow::Error::from(error));
            }
        }
        conn.query_drop("COMMIT").await?;
        Ok(())
    }
    .await;
    release_mysql_migration_lock(&mut conn, execution).await
}

async fn apply_postgres_migration(
    adapter: &PostgresAdapter,
    name: &str,
    statements: &[String],
    checksum: &str,
) -> anyhow::Result<bool> {
    let mut conn = adapter.pool.get().await.context("failed to get postgres migration connection from pool")?;
    let transaction = conn.transaction().await?;
    transaction
        .query_one("SELECT pg_advisory_xact_lock($1)", &[&POSTGRES_MIGRATION_LOCK_ID])
        .await
        .context("failed to acquire the PostgreSQL migration lock")?;
    transaction
        .batch_execute(
            "CREATE TABLE IF NOT EXISTS dinoco_migrations (
                name VARCHAR(255) PRIMARY KEY,
                applied_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
            );
            CREATE TABLE IF NOT EXISTS dinoco_migration_checksums (
                name VARCHAR(255) PRIMARY KEY,
                checksum VARCHAR(64) NOT NULL
            );
            CREATE INDEX IF NOT EXISTS dinoco_migrations_checksum_required ON dinoco_migrations(name);",
        )
        .await?;

    if transaction.query_opt("SELECT 1 FROM dinoco_migrations WHERE name = $1", &[&name]).await?.is_some() {
        let recorded = transaction
            .query_opt("SELECT checksum FROM dinoco_migration_checksums WHERE name = $1", &[&name])
            .await?
            .map(|row| row.get::<_, String>(0));
        verify_concurrent_checksum(name, checksum, recorded.as_deref())?;
        transaction.commit().await?;
        return Ok(false);
    }

    for statement in statements {
        transaction
            .batch_execute(statement)
            .await
            .with_context(|| format!("failed to execute PostgreSQL migration `{name}` statement"))?;
    }
    transaction.execute("INSERT INTO dinoco_migrations (name) VALUES ($1)", &[&name]).await?;
    transaction
        .execute("INSERT INTO dinoco_migration_checksums (name, checksum) VALUES ($1, $2)", &[&name, &checksum])
        .await?;
    transaction.commit().await?;
    Ok(true)
}

async fn apply_mysql_migration(
    adapter: &MySqlAdapter,
    name: &str,
    statements: &[String],
    checksum: &str,
) -> anyhow::Result<bool> {
    let mut conn = adapter.pool.get_conn().await.context("failed to get mysql migration connection from pool")?;
    acquire_mysql_migration_lock(&mut conn).await?;
    let application = async {
        conn.query_drop(
            "CREATE TABLE IF NOT EXISTS dinoco_migrations (
                name VARCHAR(255) PRIMARY KEY,
                applied_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
            )",
        )
        .await?;
        conn.query_drop(MYSQL_CHECKSUMS_TABLE_SQL).await?;
        ensure_mysql_checksum_guard(&mut conn).await?;

        let applied: Option<u8> = conn.exec_first("SELECT 1 FROM dinoco_migrations WHERE name = ?", (name,)).await?;
        if applied.is_some() {
            let recorded: Option<String> =
                conn.exec_first("SELECT checksum FROM dinoco_migration_checksums WHERE name = ?", (name,)).await?;
            verify_concurrent_checksum(name, checksum, recorded.as_deref())?;
            return Ok(false);
        }

        for statement in statements {
            conn.query_drop(statement)
                .await
                .with_context(|| format!("failed to execute MySQL migration `{name}` statement"))?;
        }

        let mut transaction = conn.start_transaction(TxOpts::default()).await?;
        transaction.exec_drop("INSERT INTO dinoco_migrations (name) VALUES (?)", (name,)).await?;
        transaction
            .exec_drop("INSERT INTO dinoco_migration_checksums (name, checksum) VALUES (?, ?)", (name, checksum))
            .await?;
        transaction.commit().await?;
        Ok(true)
    }
    .await;
    release_mysql_migration_lock(&mut conn, application).await
}

fn verify_concurrent_checksum(name: &str, expected: &str, recorded: Option<&str>) -> anyhow::Result<()> {
    match recorded {
        Some(recorded) if recorded == expected => Ok(()),
        Some(recorded) => anyhow::bail!(
            "Migration `{name}` was applied concurrently with checksum {recorded}, but this runner validated checksum {expected}. Refusing to continue with divergent migration history."
        ),
        None => anyhow::bail!(
            "Migration `{name}` was applied concurrently without a checksum record. Refusing to continue with unverifiable migration history."
        ),
    }
}

async fn postgres_migration_metadata(adapter: &PostgresAdapter) -> anyhow::Result<MigrationMetadata> {
    let mut conn = adapter.pool.get().await.context("failed to get postgres migration metadata connection")?;
    let transaction = conn.transaction().await?;
    transaction
        .query_one("SELECT pg_advisory_xact_lock($1)", &[&POSTGRES_MIGRATION_LOCK_ID])
        .await
        .context("failed to acquire the PostgreSQL migration lock")?;
    let history_exists: bool =
        transaction.query_one("SELECT to_regclass('public.dinoco_migrations') IS NOT NULL", &[]).await?.get(0);
    let applied = if history_exists {
        transaction
            .query("SELECT name FROM dinoco_migrations ORDER BY name", &[])
            .await?
            .into_iter()
            .map(|row| row.get::<_, String>(0))
            .collect()
    } else {
        Vec::new()
    };
    let checksums_table_exists: bool =
        transaction.query_one("SELECT to_regclass('public.dinoco_migration_checksums') IS NOT NULL", &[]).await?.get(0);
    let checksum_guard_exists: bool = transaction
        .query_one("SELECT to_regclass('public.dinoco_migrations_checksum_required') IS NOT NULL", &[])
        .await?
        .get(0);
    let checksums = if checksums_table_exists {
        Some(
            transaction
                .query("SELECT name, checksum FROM dinoco_migration_checksums ORDER BY name", &[])
                .await?
                .into_iter()
                .map(|row| (row.get::<_, String>(0), row.get::<_, String>(1)))
                .collect(),
        )
    } else {
        None
    };
    let schema_snapshots_table_exists: bool =
        transaction.query_one("SELECT to_regclass('public.dinoco_migration_schemas') IS NOT NULL", &[]).await?.get(0);
    let schema_snapshot_guard_exists: bool = transaction
        .query_one("SELECT to_regclass('public.dinoco_migrations_schema_snapshots_required') IS NOT NULL", &[])
        .await?
        .get(0);
    let schema_snapshots = if schema_snapshots_table_exists {
        Some(
            transaction
                .query("SELECT name, schema_json FROM dinoco_migration_schemas ORDER BY name", &[])
                .await?
                .into_iter()
                .map(|row| (row.get::<_, String>(0), row.get::<_, String>(1)))
                .collect(),
        )
    } else {
        None
    };
    transaction.commit().await?;
    Ok(MigrationMetadata {
        history_exists,
        applied,
        checksums,
        checksums_required: checksums_table_exists || checksum_guard_exists,
        schema_snapshots,
        schema_snapshots_required: schema_snapshots_table_exists || schema_snapshot_guard_exists,
    })
}

async fn mysql_migration_metadata(adapter: &MySqlAdapter) -> anyhow::Result<MigrationMetadata> {
    let mut conn = adapter.pool.get_conn().await.context("failed to get mysql migration metadata connection")?;
    acquire_mysql_migration_lock(&mut conn).await?;
    let inspection = async {
        let history_exists: Option<u8> = conn
            .query_first(
                "SELECT 1 FROM information_schema.tables
                 WHERE table_schema = DATABASE() AND table_name = 'dinoco_migrations'",
            )
            .await?;
        let applied = if history_exists.is_some() {
            conn.query("SELECT name FROM dinoco_migrations ORDER BY name").await?
        } else {
            Vec::new()
        };
        let checksums_table_exists: Option<u8> = conn
            .query_first(
                "SELECT 1 FROM information_schema.tables
                 WHERE table_schema = DATABASE() AND table_name = 'dinoco_migration_checksums'",
            )
            .await?;
        let checksum_guard_exists: Option<u8> = conn
            .query_first(
                "SELECT 1 FROM information_schema.statistics
                 WHERE table_schema = DATABASE()
                   AND table_name = 'dinoco_migrations'
                   AND index_name = 'dinoco_migrations_checksum_required'",
            )
            .await?;
        let checksums = if checksums_table_exists.is_some() {
            let rows: Vec<(String, String)> =
                conn.query("SELECT name, checksum FROM dinoco_migration_checksums ORDER BY name").await?;
            Some(rows.into_iter().collect())
        } else {
            None
        };
        let schema_snapshots_table_exists: Option<u8> = conn
            .query_first(
                "SELECT 1 FROM information_schema.tables
                 WHERE table_schema = DATABASE() AND table_name = 'dinoco_migration_schemas'",
            )
            .await?;
        let schema_snapshot_guard_exists: Option<u8> = conn
            .query_first(
                "SELECT 1 FROM information_schema.statistics
                 WHERE table_schema = DATABASE()
                   AND table_name = 'dinoco_migrations'
                   AND index_name = 'dinoco_migrations_schema_snapshots_required'",
            )
            .await?;
        let schema_snapshots = if schema_snapshots_table_exists.is_some() {
            let rows: Vec<(String, String)> =
                conn.query("SELECT name, schema_json FROM dinoco_migration_schemas ORDER BY name").await?;
            Some(rows.into_iter().collect())
        } else {
            None
        };
        Ok(MigrationMetadata {
            history_exists: history_exists.is_some(),
            applied,
            checksums,
            checksums_required: checksums_table_exists.is_some() || checksum_guard_exists.is_some(),
            schema_snapshots,
            schema_snapshots_required: schema_snapshots_table_exists.is_some()
                || schema_snapshot_guard_exists.is_some(),
        })
    }
    .await;
    release_mysql_migration_lock(&mut conn, inspection).await
}

async fn record_postgres_legacy_checksums(
    adapter: &PostgresAdapter,
    checksums: &[(String, String)],
) -> anyhow::Result<()> {
    let mut conn = adapter.pool.get().await.context("failed to get postgres migration connection from pool")?;
    let transaction = conn.transaction().await?;
    transaction.query_one("SELECT pg_advisory_xact_lock($1)", &[&POSTGRES_MIGRATION_LOCK_ID]).await?;
    transaction.batch_execute(POSTGRES_CHECKSUMS_TABLE_SQL).await?;
    transaction.batch_execute(POSTGRES_CHECKSUM_GUARD_SQL).await?;
    for (name, checksum) in checksums {
        if transaction.query_opt("SELECT 1 FROM dinoco_migrations WHERE name = $1", &[name]).await?.is_none() {
            anyhow::bail!("Cannot record a checksum for unapplied migration `{name}`.");
        }
        let recorded = transaction
            .query_opt("SELECT checksum FROM dinoco_migration_checksums WHERE name = $1", &[name])
            .await?
            .map(|row| row.get::<_, String>(0));
        if let Some(recorded) = recorded {
            if recorded != *checksum {
                anyhow::bail!(
                    "Migration `{name}` already has checksum {recorded}, which differs from validated checksum {checksum}."
                );
            }
        } else {
            transaction
                .execute("INSERT INTO dinoco_migration_checksums (name, checksum) VALUES ($1, $2)", &[name, checksum])
                .await?;
        }
    }
    transaction.commit().await?;
    Ok(())
}

async fn record_mysql_legacy_checksums(adapter: &MySqlAdapter, checksums: &[(String, String)]) -> anyhow::Result<()> {
    let mut conn = adapter.pool.get_conn().await.context("failed to get mysql migration connection from pool")?;
    acquire_mysql_migration_lock(&mut conn).await?;
    let recording = async {
        conn.query_drop(MYSQL_CHECKSUMS_TABLE_SQL).await?;
        ensure_mysql_checksum_guard(&mut conn).await?;
        let mut transaction = conn.start_transaction(TxOpts::default()).await?;
        for (name, checksum) in checksums {
            let applied: Option<u8> = transaction
                .exec_first("SELECT 1 FROM dinoco_migrations WHERE name = ?", (name,))
                .await?;
            if applied.is_none() {
                anyhow::bail!("Cannot record a checksum for unapplied migration `{name}`.");
            }
            let recorded: Option<String> = transaction
                .exec_first(
                    "SELECT checksum FROM dinoco_migration_checksums WHERE name = ?",
                    (name,),
                )
                .await?;
            if let Some(recorded) = recorded {
                if recorded != *checksum {
                    anyhow::bail!(
                        "Migration `{name}` already has checksum {recorded}, which differs from validated checksum {checksum}."
                    );
                }
            } else {
                transaction
                    .exec_drop(
                        "INSERT INTO dinoco_migration_checksums (name, checksum) VALUES (?, ?)",
                        (name, checksum),
                    )
                    .await?;
            }
        }
        transaction.commit().await?;
        Ok(())
    }
    .await;
    release_mysql_migration_lock(&mut conn, recording).await
}

async fn record_postgres_schema_snapshot(
    adapter: &PostgresAdapter,
    name: &str,
    schema_json: &str,
) -> anyhow::Result<()> {
    let mut conn = adapter.pool.get().await.context("failed to get postgres migration connection from pool")?;
    let transaction = conn.transaction().await?;
    transaction.query_one("SELECT pg_advisory_xact_lock($1)", &[&POSTGRES_MIGRATION_LOCK_ID]).await?;
    if transaction.query_opt("SELECT 1 FROM dinoco_migrations WHERE name = $1", &[&name]).await?.is_none() {
        anyhow::bail!("Cannot record a schema snapshot for unapplied migration `{name}`.");
    }
    transaction.batch_execute(POSTGRES_SCHEMA_SNAPSHOTS_TABLE_SQL).await?;
    transaction.batch_execute(POSTGRES_SCHEMA_SNAPSHOT_GUARD_SQL).await?;
    let recorded = transaction
        .query_opt("SELECT schema_json FROM dinoco_migration_schemas WHERE name = $1", &[&name])
        .await?
        .map(|row| row.get::<_, String>(0));
    match recorded {
        Some(recorded) if recorded != schema_json => {
            anyhow::bail!("Migration `{name}` already has a different canonical schema snapshot.");
        }
        Some(_) => {}
        None => {
            transaction
                .execute(
                    "INSERT INTO dinoco_migration_schemas (name, schema_json) VALUES ($1, $2)",
                    &[&name, &schema_json],
                )
                .await?;
        }
    }
    transaction.commit().await?;
    Ok(())
}

async fn record_mysql_schema_snapshot(adapter: &MySqlAdapter, name: &str, schema_json: &str) -> anyhow::Result<()> {
    let mut conn = adapter.pool.get_conn().await.context("failed to get mysql migration connection from pool")?;
    acquire_mysql_migration_lock(&mut conn).await?;
    let recording = async {
        let applied: Option<u8> = conn.exec_first("SELECT 1 FROM dinoco_migrations WHERE name = ?", (name,)).await?;
        if applied.is_none() {
            anyhow::bail!("Cannot record a schema snapshot for unapplied migration `{name}`.");
        }
        conn.query_drop(MYSQL_SCHEMA_SNAPSHOTS_TABLE_SQL).await?;
        ensure_mysql_schema_snapshot_guard(&mut conn).await?;
        let recorded: Option<String> =
            conn.exec_first("SELECT schema_json FROM dinoco_migration_schemas WHERE name = ?", (name,)).await?;
        match recorded {
            Some(recorded) if recorded != schema_json => {
                anyhow::bail!("Migration `{name}` already has a different canonical schema snapshot.");
            }
            Some(_) => {}
            None => {
                conn.exec_drop(
                    "INSERT INTO dinoco_migration_schemas (name, schema_json) VALUES (?, ?)",
                    (name, schema_json),
                )
                .await?;
            }
        }
        Ok(())
    }
    .await;
    release_mysql_migration_lock(&mut conn, recording).await
}

async fn acquire_mysql_migration_lock(conn: &mut dinoco_engine::mysql_async::Conn) -> anyhow::Result<()> {
    let acquired: Option<u8> = conn.query_first(MYSQL_MIGRATION_LOCK_SQL).await?;
    if acquired != Some(1) {
        anyhow::bail!("Timed out waiting for the MySQL migration lock.");
    }
    Ok(())
}

async fn ensure_mysql_checksum_guard(conn: &mut dinoco_engine::mysql_async::Conn) -> anyhow::Result<()> {
    let exists: Option<u8> = conn
        .query_first(
            "SELECT 1 FROM information_schema.statistics
             WHERE table_schema = DATABASE()
               AND table_name = 'dinoco_migrations'
               AND index_name = 'dinoco_migrations_checksum_required'",
        )
        .await?;
    if exists.is_none() {
        conn.query_drop("CREATE INDEX dinoco_migrations_checksum_required ON dinoco_migrations(name)").await?;
    }
    Ok(())
}

async fn ensure_mysql_schema_snapshot_guard(conn: &mut dinoco_engine::mysql_async::Conn) -> anyhow::Result<()> {
    let exists: Option<u8> = conn
        .query_first(
            "SELECT 1 FROM information_schema.statistics
             WHERE table_schema = DATABASE()
               AND table_name = 'dinoco_migrations'
               AND index_name = 'dinoco_migrations_schema_snapshots_required'",
        )
        .await?;
    if exists.is_none() {
        conn.query_drop("CREATE INDEX dinoco_migrations_schema_snapshots_required ON dinoco_migrations(name)").await?;
    }
    Ok(())
}

async fn release_mysql_migration_lock<T>(
    conn: &mut dinoco_engine::mysql_async::Conn,
    result: anyhow::Result<T>,
) -> anyhow::Result<T> {
    let released: anyhow::Result<Option<u8>> = conn.query_first(MYSQL_MIGRATION_UNLOCK_SQL).await.map_err(Into::into);
    match (result, released) {
        (Ok(value), Ok(Some(1))) => Ok(value),
        (Ok(_), Ok(_)) => anyhow::bail!("MySQL migration lock was lost before it could be released."),
        (Ok(_), Err(error)) => Err(error.context("failed to release the MySQL migration lock")),
        (Err(error), _) => Err(error),
    }
}

async fn inspect_sqlite(adapter: &SqliteAdapter) -> anyhow::Result<DatabaseSchema> {
    let conn = adapter.pool.get().await.context("failed to get sqlite connection from pool")?;

    conn.interact(move |conn| -> anyhow::Result<DatabaseSchema> {
        let mut tables_stmt = conn.prepare(
            "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' AND name NOT IN ('dinoco_migrations', 'dinoco_migration_checksums', 'dinoco_migration_schemas') ORDER BY name",
        )?;
        let table_names = tables_stmt
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        let mut tables = Vec::new();

        for table_name in table_names {
            let row_count =
                conn.query_row(&format!("SELECT COUNT(*) FROM {table_name}"), [], |row| row.get::<_, i64>(0))?;
            let mut columns_stmt = conn.prepare(&format!("PRAGMA table_info({table_name})"))?;
            let mut columns = columns_stmt.query_map([], sqlite_column)?.collect::<Result<Vec<_>, _>>()?;
            let unique_columns = sqlite_unique_columns(conn, &table_name)?;
            for column in &mut columns {
                column.unique = unique_columns.contains(column.name.as_str());
            }
            let foreign_keys = sqlite_foreign_keys(conn, &table_name)?;
            let indexes = sqlite_indexes(conn, &table_name)?;

            tables.push(DatabaseTable { name: table_name, row_count, columns, foreign_keys, indexes });
        }

        Ok(DatabaseSchema { tables, enums: Vec::new() })
    })
    .await
    .map_err(|err| anyhow::anyhow!(err.to_string()))?
}

fn sqlite_foreign_keys(conn: &rusqlite::Connection, table_name: &str) -> rusqlite::Result<Vec<MigrationForeignKey>> {
    let mut stmt = conn.prepare(&format!("PRAGMA foreign_key_list({table_name})"))?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;

    let mut grouped: Vec<MigrationForeignKey> = Vec::new();
    let mut rows = rows;
    rows.sort_by_key(|(id, sequence, ..)| (*id, *sequence));
    for (id, _, references_table, column, references_column, on_update, on_delete) in rows {
        let action_update = parse_referential_action(&on_update);
        let action_delete = parse_referential_action(&on_delete);
        if let Some(foreign_key) =
            grouped.iter_mut().find(|foreign_key| foreign_key.name == sqlite_fk_name(table_name, id))
        {
            foreign_key.columns.push(column);
            foreign_key.references_columns.push(references_column);
        } else {
            grouped.push(MigrationForeignKey {
                name: sqlite_fk_name(table_name, id),
                columns: vec![column],
                references_table,
                references_columns: vec![references_column],
                on_update: action_update,
                on_delete: action_delete,
            });
        }
    }

    for foreign_key in &mut grouped {
        foreign_key.name = generated_fk_name(table_name, &foreign_key.columns);
    }

    Ok(grouped)
}

fn sqlite_column(row: &rusqlite::Row<'_>) -> rusqlite::Result<MigrationColumn> {
    let name: String = row.get(1)?;
    let raw_type: String = row.get(2)?;
    let not_null: i64 = row.get(3)?;
    let default: Option<String> = row.get(4)?;
    let primary_key: i64 = row.get(5)?;
    let ty = parse_column_type(&raw_type);
    let mut default = default.and_then(|value| parse_default_for_type(&value, &ty));
    if default.is_none() && primary_key > 0 && matches!(ty, MigrationColumnType::Integer) {
        // An INTEGER PRIMARY KEY is an alias for SQLite's rowid and generates an integer
        // when omitted, even when the optional AUTOINCREMENT keyword is absent.
        default = Some(dinoco_engine::MigrationDefault::AutoIncrement);
    }

    Ok(MigrationColumn {
        name,
        default,
        ty,
        primary_key: primary_key > 0,
        unique: false,
        nullable: not_null == 0 && primary_key == 0,
    })
}

fn sqlite_unique_columns(
    conn: &rusqlite::Connection,
    table_name: &str,
) -> rusqlite::Result<std::collections::BTreeSet<String>> {
    let mut index_stmt =
        conn.prepare("SELECT name FROM pragma_index_list(?1) WHERE [unique] = 1 AND origin <> 'pk' ORDER BY seq")?;
    let index_names =
        index_stmt.query_map([table_name], |row| row.get::<_, String>(0))?.collect::<Result<Vec<_>, _>>()?;
    let mut unique_columns = std::collections::BTreeSet::new();

    for index_name in index_names {
        let mut columns_stmt = conn.prepare("SELECT name FROM pragma_index_info(?1) ORDER BY seqno")?;
        let columns =
            columns_stmt.query_map([index_name], |row| row.get::<_, String>(0))?.collect::<Result<Vec<_>, _>>()?;
        if let [column] = columns.as_slice() {
            unique_columns.insert(column.clone());
        }
    }

    Ok(unique_columns)
}

fn sqlite_indexes(conn: &rusqlite::Connection, table_name: &str) -> rusqlite::Result<Vec<MigrationIndex>> {
    let mut index_stmt =
        conn.prepare("SELECT name, [unique] FROM pragma_index_list(?1) WHERE origin = 'c' ORDER BY name")?;
    let index_names = index_stmt
        .query_map([table_name], |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)))?
        .collect::<Result<Vec<_>, _>>()?;
    let mut indexes = Vec::new();

    for (name, unique) in index_names {
        let mut columns_stmt = conn.prepare("SELECT name FROM pragma_index_info(?1) ORDER BY seqno")?;
        let columns =
            columns_stmt.query_map([name.as_str()], |row| row.get::<_, String>(0))?.collect::<Result<Vec<_>, _>>()?;
        if !columns.is_empty() && !(unique == 1 && columns.len() == 1) {
            indexes.push(MigrationIndex {
                name,
                columns,
                automatic: false,
                kind: if unique == 1 { MigrationIndexKind::Unique } else { MigrationIndexKind::Standard },
            });
        }
    }

    Ok(indexes)
}

async fn inspect_postgres(adapter: &PostgresAdapter) -> anyhow::Result<DatabaseSchema> {
    let conn = adapter.pool.get().await.context("failed to get postgres connection from pool")?;
    let table_rows = conn
        .query(
            "SELECT table_name FROM information_schema.tables WHERE table_schema = 'public' AND table_type = 'BASE TABLE' AND table_name NOT IN ('dinoco_migrations', 'dinoco_migration_checksums', 'dinoco_migration_schemas') ORDER BY table_name",
            &[],
        )
        .await?;
    let mut tables = Vec::new();

    for row in table_rows {
        let table_name: String = row.try_get(0)?;
        let count_sql = format!("SELECT COUNT(*) FROM {table_name}");
        let row_count: i64 = conn.query_one(&count_sql, &[]).await?.try_get(0)?;
        let column_rows = conn
            .query(
                "SELECT c.column_name, c.data_type, c.is_nullable, c.column_default,
                        CASE WHEN kcu.column_name IS NULL THEN false ELSE true END AS primary_key,
                        c.udt_name,
                        c.is_identity,
                        EXISTS (
                            SELECT 1
                            FROM information_schema.table_constraints utc
                            JOIN information_schema.key_column_usage ukcu
                              ON ukcu.constraint_schema = utc.constraint_schema
                             AND ukcu.constraint_name = utc.constraint_name
                             AND ukcu.table_name = utc.table_name
                            WHERE utc.table_schema = c.table_schema
                              AND utc.table_name = c.table_name
                              AND utc.constraint_type = 'UNIQUE'
                              AND ukcu.column_name = c.column_name
                              AND (
                                  SELECT COUNT(*)
                                  FROM information_schema.key_column_usage ukcu_count
                                  WHERE ukcu_count.constraint_schema = utc.constraint_schema
                                    AND ukcu_count.constraint_name = utc.constraint_name
                                    AND ukcu_count.table_name = utc.table_name
                              ) = 1
                        ) AS unique_column
                 FROM information_schema.columns c
                 LEFT JOIN information_schema.table_constraints tc ON tc.table_schema = c.table_schema AND tc.table_name = c.table_name AND tc.constraint_type = 'PRIMARY KEY'
                 LEFT JOIN information_schema.key_column_usage kcu ON kcu.constraint_name = tc.constraint_name AND kcu.table_schema = c.table_schema AND kcu.table_name = c.table_name AND kcu.column_name = c.column_name
                 WHERE c.table_schema = 'public' AND c.table_name = $1
                 ORDER BY c.ordinal_position",
                &[&table_name],
            )
            .await?;
        let columns = column_rows
            .into_iter()
            .map(|row| -> anyhow::Result<MigrationColumn> {
                let raw_type: String = row.try_get(1)?;
                let udt_name: String = row.try_get(5)?;
                let nullable: String = row.try_get(2)?;
                let default: Option<String> = row.try_get(3)?;
                let is_identity: String = row.try_get(6)?;
                let ty = if raw_type == "USER-DEFINED" {
                    MigrationColumnType::Enum { name: udt_name, values: Vec::new() }
                } else {
                    parse_column_type(&raw_type)
                };
                Ok(MigrationColumn {
                    name: row.try_get(0)?,
                    default: if is_identity == "YES" {
                        Some(dinoco_engine::MigrationDefault::AutoIncrement)
                    } else {
                        default.and_then(|value| parse_default_for_type(&value, &ty))
                    },
                    ty,
                    primary_key: row.try_get(4)?,
                    unique: row.try_get(7)?,
                    nullable: nullable == "YES",
                })
            })
            .collect::<anyhow::Result<Vec<_>>>()?;
        let foreign_keys = postgres_foreign_keys(&conn, &table_name).await?;
        let indexes = postgres_indexes(&conn, &table_name, &columns).await?;

        tables.push(DatabaseTable { name: table_name, row_count, columns, foreign_keys, indexes });
    }

    let enum_rows = conn
        .query(
            "SELECT t.typname, e.enumlabel FROM pg_type t JOIN pg_enum e ON e.enumtypid = t.oid JOIN pg_namespace n ON n.oid = t.typnamespace WHERE n.nspname = 'public' ORDER BY t.typname, e.enumsortorder",
            &[],
        )
        .await?;
    let enums = fold_enum_rows(enum_rows.into_iter().map(|row| Ok((row.try_get(0)?, row.try_get(1)?))))?;
    for column in tables.iter_mut().flat_map(|table| &mut table.columns) {
        if let MigrationColumnType::Enum { name, values } = &mut column.ty
            && let Some(item) = enums.iter().find(|item| item.name == *name)
        {
            *values = item.values.clone();
        }
    }

    Ok(DatabaseSchema { tables, enums })
}

async fn postgres_foreign_keys(
    conn: &dinoco_engine::deadpool_postgres::Client,
    table_name: &str,
) -> anyhow::Result<Vec<MigrationForeignKey>> {
    let rows = conn
        .query(
            "SELECT tc.constraint_name, kcu.column_name,
                    referenced_kcu.table_name AS foreign_table_name,
                    referenced_kcu.column_name AS foreign_column_name,
                    rc.update_rule, rc.delete_rule
             FROM information_schema.table_constraints tc
             JOIN information_schema.key_column_usage kcu ON kcu.constraint_schema = tc.constraint_schema AND kcu.constraint_name = tc.constraint_name AND kcu.table_name = tc.table_name
             JOIN information_schema.referential_constraints rc ON rc.constraint_schema = tc.constraint_schema AND rc.constraint_name = tc.constraint_name
             JOIN information_schema.key_column_usage referenced_kcu
               ON referenced_kcu.constraint_schema = rc.unique_constraint_schema
              AND referenced_kcu.constraint_name = rc.unique_constraint_name
              AND referenced_kcu.ordinal_position = kcu.position_in_unique_constraint
             WHERE tc.constraint_type = 'FOREIGN KEY' AND tc.table_schema = 'public' AND tc.table_name = $1
             ORDER BY tc.constraint_name, kcu.ordinal_position",
            &[&table_name],
        )
        .await?;

    let mut grouped: Vec<MigrationForeignKey> = Vec::new();
    for row in rows {
        let name: String = row.try_get(0)?;
        let column: String = row.try_get(1)?;
        let references_table: String = row.try_get(2)?;
        let references_column: String = row.try_get(3)?;
        let on_update: String = row.try_get(4)?;
        let on_delete: String = row.try_get(5)?;

        if let Some(foreign_key) = grouped.iter_mut().find(|foreign_key| foreign_key.name == name) {
            foreign_key.columns.push(column);
            foreign_key.references_columns.push(references_column);
        } else {
            grouped.push(MigrationForeignKey {
                name,
                columns: vec![column],
                references_table,
                references_columns: vec![references_column],
                on_update: parse_referential_action(&on_update),
                on_delete: parse_referential_action(&on_delete),
            });
        }
    }

    Ok(grouped)
}

async fn postgres_indexes(
    conn: &dinoco_engine::deadpool_postgres::Client,
    table_name: &str,
    columns: &[MigrationColumn],
) -> anyhow::Result<Vec<MigrationIndex>> {
    let rows = conn
        .query(
            "SELECT i.relname, a.attname, ix.indisunique
             FROM pg_class t
             JOIN pg_namespace n ON n.oid = t.relnamespace
             JOIN pg_index ix ON ix.indrelid = t.oid
             JOIN pg_class i ON i.oid = ix.indexrelid
             JOIN LATERAL unnest(ix.indkey) WITH ORDINALITY AS key(attnum, ordinality) ON true
             JOIN pg_attribute a ON a.attrelid = t.oid AND a.attnum = key.attnum
             LEFT JOIN pg_constraint c ON c.conindid = i.oid
             WHERE n.nspname = 'public'
               AND t.relname = $1
               AND NOT ix.indisprimary
               AND ix.indpred IS NULL
               AND key.attnum > 0
               AND c.oid IS NULL
             ORDER BY i.relname, key.ordinality",
            &[&table_name],
        )
        .await?;
    let mut indexes: Vec<MigrationIndex> = Vec::new();

    for row in rows {
        let name: String = row.try_get(0)?;
        let column: String = row.try_get(1)?;
        let unique: bool = row.try_get(2)?;
        if let Some(index) = indexes.iter_mut().find(|index| index.name == name) {
            index.columns.push(column);
        } else {
            indexes.push(MigrationIndex {
                name,
                columns: vec![column],
                automatic: false,
                kind: if unique { MigrationIndexKind::Unique } else { MigrationIndexKind::Standard },
            });
        }
    }
    indexes.retain(|index| index.kind != MigrationIndexKind::Unique || index.columns.len() > 1);

    let fulltext_rows = conn
        .query(
            "SELECT i.relname, pg_get_indexdef(i.oid)
             FROM pg_class t
             JOIN pg_namespace n ON n.oid = t.relnamespace
             JOIN pg_index ix ON ix.indrelid = t.oid
             JOIN pg_class i ON i.oid = ix.indexrelid
             JOIN pg_am am ON am.oid = i.relam
             LEFT JOIN pg_constraint c ON c.conindid = i.oid
             WHERE n.nspname = 'public'
               AND t.relname = $1
               AND am.amname = 'gin'
               AND ix.indexprs IS NOT NULL
               AND ix.indpred IS NULL
               AND c.oid IS NULL
             ORDER BY i.relname",
            &[&table_name],
        )
        .await?;

    for row in fulltext_rows {
        let name: String = row.try_get(0)?;
        let definition: String = row.try_get(1)?;
        let indexed_columns = postgres_fulltext_columns(&definition, columns);
        if !indexed_columns.is_empty() {
            indexes.push(MigrationIndex {
                name,
                columns: indexed_columns,
                automatic: false,
                kind: MigrationIndexKind::FullText,
            });
        }
    }

    Ok(indexes)
}

fn postgres_fulltext_columns(definition: &str, columns: &[MigrationColumn]) -> Vec<String> {
    let mut columns = columns
        .iter()
        .filter_map(|column| {
            let plain = format!("COALESCE({},", column.name);
            let quoted = format!("COALESCE(\"{}\",", column.name.replace('"', "\"\""));
            definition
                .find(&plain)
                .into_iter()
                .chain(definition.find(&quoted))
                .min()
                .map(|position| (position, column.name.clone()))
        })
        .collect::<Vec<_>>();
    columns.sort_by_key(|(position, _)| *position);
    columns.into_iter().map(|(_, column)| column).collect()
}

async fn inspect_pgbouncer(adapter: &PgBouncerAdapter) -> anyhow::Result<DatabaseSchema> {
    inspect_postgres(adapter.inner()).await
}

async fn inspect_mysql(adapter: &MySqlAdapter) -> anyhow::Result<DatabaseSchema> {
    let mut conn = adapter.pool.get_conn().await.context("failed to get mysql connection from pool")?;
    let table_names: Vec<String> = conn
        .query(
            "SELECT table_name FROM information_schema.tables WHERE table_schema = DATABASE() AND table_type = 'BASE TABLE' AND table_name NOT IN ('dinoco_migrations', 'dinoco_migration_checksums', 'dinoco_migration_schemas') ORDER BY table_name",
        )
        .await?;
    let mut tables = Vec::new();

    for table_name in table_names {
        let row_count: Option<i64> = conn.query_first(format!("SELECT COUNT(*) FROM {table_name}")).await?;
        let rows: Vec<dinoco_engine::mysql_async::Row> = conn
            .exec(
                "SELECT column_name AS name, data_type AS raw_type, column_type AS column_type,
                        is_nullable AS nullable, column_default AS default_value,
                        column_key AS column_key, extra AS extra
                 FROM information_schema.columns
                 WHERE table_schema = DATABASE() AND table_name = ?
                 ORDER BY ordinal_position",
                (table_name.clone(),),
            )
            .await?;
        let columns = rows
            .into_iter()
            .map(|mut row| {
                let raw_type = row.take::<String, _>("raw_type").unwrap_or_default();
                let column_type = row.take::<String, _>("column_type").unwrap_or_default();
                let nullable = row.take::<String, _>("nullable").unwrap_or_default();
                let key = row.take::<String, _>("column_key").unwrap_or_default();
                let extra = row.take::<String, _>("extra").unwrap_or_default();
                let default = row.take::<Option<String>, _>("default_value").flatten();
                let ty = if raw_type == "enum" {
                    MigrationColumnType::Enum { name: String::new(), values: parse_mysql_enum_values(&column_type) }
                } else if raw_type == "tinyint" && column_type.to_ascii_lowercase().starts_with("tinyint(1)") {
                    MigrationColumnType::Boolean
                } else {
                    parse_column_type(&raw_type)
                };
                MigrationColumn {
                    name: row.take::<String, _>("name").unwrap_or_default(),
                    default: if extra.to_ascii_lowercase().contains("auto_increment") {
                        Some(dinoco_engine::MigrationDefault::AutoIncrement)
                    } else {
                        default.and_then(|value| parse_default_for_type(&value, &ty))
                    },
                    ty,
                    primary_key: key == "PRI",
                    unique: key == "UNI",
                    nullable: nullable == "YES",
                }
            })
            .collect();
        let foreign_keys = mysql_foreign_keys(&mut conn, &table_name).await?;
        let indexes = mysql_indexes(&mut conn, &table_name).await?;

        tables.push(DatabaseTable {
            name: table_name,
            row_count: row_count.unwrap_or_default(),
            columns,
            foreign_keys,
            indexes,
        });
    }

    Ok(DatabaseSchema { tables, enums: Vec::new() })
}

async fn mysql_foreign_keys(
    conn: &mut dinoco_engine::mysql_async::Conn,
    table_name: &str,
) -> anyhow::Result<Vec<MigrationForeignKey>> {
    let rows: Vec<dinoco_engine::mysql_async::Row> = conn
        .exec(
            "SELECT kcu.constraint_name AS name, kcu.column_name AS column_name, kcu.referenced_table_name AS referenced_table, kcu.referenced_column_name AS referenced_column, rc.update_rule AS update_rule, rc.delete_rule AS delete_rule
             FROM information_schema.key_column_usage kcu
             JOIN information_schema.referential_constraints rc ON rc.constraint_schema = kcu.constraint_schema AND rc.constraint_name = kcu.constraint_name
             WHERE kcu.table_schema = DATABASE() AND kcu.table_name = ? AND kcu.referenced_table_name IS NOT NULL
             ORDER BY kcu.constraint_name, kcu.ordinal_position",
            (table_name.to_string(),),
        )
        .await?;

    let mut grouped: Vec<MigrationForeignKey> = Vec::new();
    for mut row in rows {
        let name = row.take::<String, _>("name").unwrap_or_default();
        let column = row.take::<String, _>("column_name").unwrap_or_default();
        let references_table = row.take::<String, _>("referenced_table").unwrap_or_default();
        let references_column = row.take::<String, _>("referenced_column").unwrap_or_default();
        let on_update = row.take::<String, _>("update_rule").unwrap_or_default();
        let on_delete = row.take::<String, _>("delete_rule").unwrap_or_default();

        if let Some(foreign_key) = grouped.iter_mut().find(|foreign_key| foreign_key.name == name) {
            foreign_key.columns.push(column);
            foreign_key.references_columns.push(references_column);
        } else {
            grouped.push(MigrationForeignKey {
                name,
                columns: vec![column],
                references_table,
                references_columns: vec![references_column],
                on_update: parse_referential_action(&on_update),
                on_delete: parse_referential_action(&on_delete),
            });
        }
    }

    Ok(grouped)
}

async fn mysql_indexes(
    conn: &mut dinoco_engine::mysql_async::Conn,
    table_name: &str,
) -> anyhow::Result<Vec<MigrationIndex>> {
    let rows: Vec<dinoco_engine::mysql_async::Row> = conn
        .exec(
            "SELECT index_name AS name, column_name AS column_name, index_type AS index_type,
                    non_unique AS non_unique
             FROM information_schema.statistics
             WHERE table_schema = DATABASE()
               AND table_name = ?
               AND index_name <> 'PRIMARY'
             ORDER BY index_name, seq_in_index",
            (table_name.to_string(),),
        )
        .await?;
    let mut indexes: Vec<MigrationIndex> = Vec::new();

    for mut row in rows {
        let name = row.take::<String, _>("name").unwrap_or_default();
        let column = row.take::<String, _>("column_name").unwrap_or_default();
        let index_type = row.take::<String, _>("index_type").unwrap_or_default();
        let unique = row.take::<u8, _>("non_unique").unwrap_or(1) == 0;
        let kind = if index_type.eq_ignore_ascii_case("FULLTEXT") {
            MigrationIndexKind::FullText
        } else if unique {
            MigrationIndexKind::Unique
        } else {
            MigrationIndexKind::Standard
        };
        if name.is_empty() || column.is_empty() {
            continue;
        }
        if let Some(index) = indexes.iter_mut().find(|index| index.name == name) {
            index.columns.push(column);
        } else {
            indexes.push(MigrationIndex { name, columns: vec![column], automatic: false, kind });
        }
    }
    indexes.retain(|index| index.kind != MigrationIndexKind::Unique || index.columns.len() > 1);

    Ok(indexes)
}

fn parse_column_type(raw: &str) -> MigrationColumnType {
    let raw = raw.to_ascii_lowercase();
    if raw.contains("json") || raw.contains("blob") {
        MigrationColumnType::Json
    } else if raw == "date" {
        MigrationColumnType::Date
    } else if raw.contains("timestamp") || raw.contains("datetime") || raw.contains("time") {
        MigrationColumnType::DateTime
    } else if raw.contains("bool") || raw == "tinyint(1)" {
        MigrationColumnType::Boolean
    } else if raw.contains("int") || raw.contains("serial") {
        MigrationColumnType::Integer
    } else if raw.contains("double") || raw.contains("float") || raw.contains("real") || raw.contains("numeric") {
        MigrationColumnType::Float
    } else if raw.contains("text") || raw.contains("json") {
        MigrationColumnType::Text
    } else {
        MigrationColumnType::String
    }
}

fn parse_mysql_enum_values(raw: &str) -> Vec<String> {
    raw.trim_start_matches("enum(")
        .trim_end_matches(')')
        .split(',')
        .map(|value| value.trim().trim_matches('\'').replace("''", "'"))
        .filter(|value| !value.is_empty())
        .collect()
}

fn parse_default_for_type(raw: &str, ty: &MigrationColumnType) -> Option<dinoco_engine::MigrationDefault> {
    use dinoco_engine::MigrationDefault;

    let value = strip_wrapping_parentheses(raw.trim());
    if value.eq_ignore_ascii_case("null") {
        None
    } else if value.eq_ignore_ascii_case("current_timestamp")
        || value.eq_ignore_ascii_case("current_timestamp()")
        || value.eq_ignore_ascii_case("now()")
    {
        Some(MigrationDefault::CurrentTimestamp)
    } else if matches!(ty, MigrationColumnType::Boolean) {
        match value.to_ascii_lowercase().as_str() {
            "true" | "1" => Some(MigrationDefault::Boolean(true)),
            "false" | "0" => Some(MigrationDefault::Boolean(false)),
            _ => None,
        }
    } else if matches!(ty, MigrationColumnType::Integer) {
        value.parse::<i64>().ok().map(MigrationDefault::Integer)
    } else if matches!(ty, MigrationColumnType::Float) {
        value.parse::<f64>().ok().map(MigrationDefault::Float)
    } else {
        parse_sql_string_literal(value).map(MigrationDefault::String)
    }
}

fn strip_wrapping_parentheses(mut value: &str) -> &str {
    loop {
        let trimmed = value.trim();
        if trimmed.len() >= 2 && trimmed.starts_with('(') && trimmed.ends_with(')') {
            value = &trimmed[1..trimmed.len() - 1];
        } else {
            return trimmed;
        }
    }
}

fn parse_sql_string_literal(value: &str) -> Option<String> {
    if value.len() < 2 {
        return None;
    }

    let quote = value.as_bytes()[0] as char;
    if !matches!(quote, '\'' | '"') || value.as_bytes()[value.len() - 1] as char != quote {
        return None;
    }

    let inner = &value[1..value.len() - 1];
    let escaped = format!("{quote}{quote}");
    Some(inner.replace(&escaped, &quote.to_string()))
}

fn parse_referential_action(raw: &str) -> ReferentialAction {
    match raw.trim().replace('_', " ").to_ascii_uppercase().as_str() {
        "CASCADE" => ReferentialAction::Cascade,
        "RESTRICT" => ReferentialAction::Restrict,
        "SET NULL" => ReferentialAction::SetNull,
        "SET DEFAULT" => ReferentialAction::SetDefault,
        "NO ACTION" => ReferentialAction::NoAction,
        _ => ReferentialAction::NoAction,
    }
}

fn sqlite_migration_authorizer(context: AuthContext<'_>) -> Authorization {
    sqlite_authorization(context, true)
}

fn sqlite_custom_migration_authorizer(context: AuthContext<'_>) -> Authorization {
    sqlite_authorization(context, false)
}

fn sqlite_authorization(context: AuthContext<'_>, allow_metadata_mutation: bool) -> Authorization {
    let external_database = match context.action {
        AuthAction::AlterTable { database_name, .. } => database_name != "main" && database_name != "temp",
        _ => context.database_name.is_some_and(|name| name != "main" && name != "temp"),
    };
    let unsafe_action = match context.action {
        AuthAction::Attach { .. }
        | AuthAction::Detach { .. }
        | AuthAction::CreateVtable { .. }
        | AuthAction::DropVtable { .. }
        | AuthAction::Transaction { .. }
        | AuthAction::Savepoint { .. } => true,
        AuthAction::Pragma { pragma_name, pragma_value } => match pragma_name.to_ascii_lowercase().as_str() {
            "defer_foreign_keys" | "legacy_alter_table" | "foreign_key_check" => false,
            "foreign_keys" => pragma_value
                .is_some_and(|value| !matches!(value.to_ascii_lowercase().as_str(), "1" | "on" | "true" | "yes")),
            _ => true,
        },
        AuthAction::Function { function_name } => {
            matches!(function_name.to_ascii_lowercase().as_str(), "load_extension" | "readfile" | "writefile")
        }
        _ => false,
    };
    let metadata_mutation = !allow_metadata_mutation
        && match context.action {
            AuthAction::CreateTable { table_name }
            | AuthAction::DropTable { table_name }
            | AuthAction::Insert { table_name }
            | AuthAction::Delete { table_name } => is_migration_metadata_table(table_name),
            AuthAction::Update { table_name, .. }
            | AuthAction::CreateTrigger { table_name, .. }
            | AuthAction::DropTrigger { table_name, .. }
            | AuthAction::CreateIndex { table_name, .. }
            | AuthAction::DropIndex { table_name, .. }
            | AuthAction::AlterTable { table_name, .. } => is_migration_metadata_table(table_name),
            _ => false,
        };

    if external_database || unsafe_action || metadata_mutation { Authorization::Deny } else { Authorization::Allow }
}

fn is_migration_metadata_table(table: &str) -> bool {
    matches!(table, "dinoco_migrations" | "dinoco_migration_checksums" | "dinoco_migration_schemas")
}

fn ensure_sqlite_foreign_key_integrity(transaction: &rusqlite::Transaction<'_>) -> anyhow::Result<()> {
    let mut statement = transaction.prepare("PRAGMA foreign_key_check")?;
    let mut rows = statement.query([])?;
    if let Some(row) = rows.next()? {
        let table: String = row.get(0)?;
        let row_id: Option<i64> = row.get(1)?;
        let parent: String = row.get(2)?;
        let foreign_key: i64 = row.get(3)?;
        anyhow::bail!(
            "SQLite foreign key integrity check failed for table `{table}`, row {}, parent table `{parent}`, foreign key #{foreign_key}",
            row_id.map_or_else(|| "without rowid".to_string(), |value| value.to_string())
        );
    }
    Ok(())
}

fn ensure_sqlite_migration_checksum_guard(transaction: &rusqlite::Transaction<'_>) -> anyhow::Result<()> {
    let migrations_table_exists: bool = transaction.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'dinoco_migrations')",
        [],
        |row| row.get(0),
    )?;
    if !migrations_table_exists {
        return Ok(());
    }

    transaction
        .execute("CREATE INDEX IF NOT EXISTS dinoco_migrations_checksum_required ON dinoco_migrations(name)", [])?;
    Ok(())
}

fn escape_sql_literal(value: &str) -> String {
    value.replace('\'', "''")
}

fn generated_fk_name(table: &str, columns: &[String]) -> String {
    format!("fk_{}_{}", table, columns.join("_"))
}

fn sqlite_fk_name(table: &str, id: i64) -> String {
    format!("__sqlite_{table}_{id}")
}

fn fold_enum_rows<I>(rows: I) -> anyhow::Result<Vec<DatabaseEnum>>
where
    I: IntoIterator<Item = anyhow::Result<(String, String)>>,
{
    let mut enums: Vec<DatabaseEnum> = Vec::new();
    for row in rows {
        let (name, value) = row?;
        if let Some(item) = enums.iter_mut().find(|item| item.name == name) {
            item.values.push(value);
        } else {
            enums.push(DatabaseEnum { name, values: vec![value] });
        }
    }
    Ok(enums)
}
