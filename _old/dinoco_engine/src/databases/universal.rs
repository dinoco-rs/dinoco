use async_trait::async_trait;
use dinoco_compiler::Database;
use std::future::Future;
use std::pin::Pin;

use crate::{
    AdapterDialect, ColumnDefinition, DatabaseColumn, DatabaseEnumRaw, DatabaseForeignKey, DatabaseIndex,
    DatabaseParsedTable, DinocoAdapter, DinocoAdapterHandler, DinocoClientConfig, DinocoError, DinocoResult, DinocoRow,
    DinocoTransactionAdapter, DinocoValue, ExecutionResult, MigrationExecutor, MigrationStep, MySqlAdapter,
    PostgresAdapter, SqliteAdapter,
};

#[derive(Clone)]
pub enum UniversalAdapter {
    Mysql(MySqlAdapter),
    Postgresql(PostgresAdapter),
    Sqlite(SqliteAdapter),
}

#[derive(Clone, Copy)]
pub enum UniversalDialect {
    Mysql,
    Postgresql,
    Sqlite,
}

static MYSQL_DIALECT: UniversalDialect = UniversalDialect::Mysql;
static POSTGRESQL_DIALECT: UniversalDialect = UniversalDialect::Postgresql;
static SQLITE_DIALECT: UniversalDialect = UniversalDialect::Sqlite;

impl UniversalAdapter {
    pub async fn connect_for_database(
        database: &Database,
        url: String,
        config: DinocoClientConfig,
    ) -> DinocoResult<Self> {
        match database {
            Database::Mysql => Ok(Self::Mysql(MySqlAdapter::connect(url, config).await?)),
            Database::Postgresql => Ok(Self::Postgresql(PostgresAdapter::connect(url, config).await?)),
            Database::Sqlite => Ok(Self::Sqlite(SqliteAdapter::connect(url, config).await?)),
        }
    }
}

fn detect_database(url: &str) -> Option<Database> {
    if url.starts_with("postgres://") || url.starts_with("postgresql://") {
        return Some(Database::Postgresql);
    }

    if url.starts_with("mysql://") {
        return Some(Database::Mysql);
    }

    if url.starts_with("file:") {
        return Some(Database::Sqlite);
    }

    None
}

impl AdapterDialect for UniversalDialect {
    fn bind_param(&self, index: usize) -> String {
        match self {
            Self::Mysql => {
                let dialect = crate::MySqlDialect;
                dialect.bind_param(index)
            }
            Self::Postgresql => {
                let dialect = crate::PostgresDialect;
                dialect.bind_param(index)
            }
            Self::Sqlite => {
                let dialect = crate::SqliteDialect;
                dialect.bind_param(index)
            }
        }
    }

    fn bind_value(&self, index: usize, value: &DinocoValue) -> String {
        match self {
            Self::Mysql => {
                let dialect = crate::MySqlDialect;
                dialect.bind_value(index, value)
            }
            Self::Postgresql => {
                let dialect = crate::PostgresDialect;
                dialect.bind_value(index, value)
            }
            Self::Sqlite => {
                let dialect = crate::SqliteDialect;
                dialect.bind_value(index, value)
            }
        }
    }

    fn cast_numeric_for_division(&self, expr: &str) -> String {
        match self {
            Self::Mysql => {
                let dialect = crate::MySqlDialect;
                dialect.cast_numeric_for_division(expr)
            }
            Self::Postgresql => {
                let dialect = crate::PostgresDialect;
                dialect.cast_numeric_for_division(expr)
            }
            Self::Sqlite => {
                let dialect = crate::SqliteDialect;
                dialect.cast_numeric_for_division(expr)
            }
        }
    }

    fn identifier(&self, value: &str) -> String {
        match self {
            Self::Mysql => {
                let dialect = crate::MySqlDialect;
                dialect.identifier(value)
            }
            Self::Postgresql => {
                let dialect = crate::PostgresDialect;
                dialect.identifier(value)
            }
            Self::Sqlite => {
                let dialect = crate::SqliteDialect;
                dialect.identifier(value)
            }
        }
    }

    fn literal_string(&self, value: &str) -> String {
        match self {
            Self::Mysql => {
                let dialect = crate::MySqlDialect;
                dialect.literal_string(value)
            }
            Self::Postgresql => {
                let dialect = crate::PostgresDialect;
                dialect.literal_string(value)
            }
            Self::Sqlite => {
                let dialect = crate::SqliteDialect;
                dialect.literal_string(value)
            }
        }
    }

    fn column_type(&self, definition: &ColumnDefinition, is_primary: bool, auto_increment: bool) -> String {
        match self {
            Self::Mysql => {
                let dialect = crate::MySqlDialect;
                dialect.column_type(definition, is_primary, auto_increment)
            }
            Self::Postgresql => {
                let dialect = crate::PostgresDialect;
                dialect.column_type(definition, is_primary, auto_increment)
            }
            Self::Sqlite => {
                let dialect = crate::SqliteDialect;
                dialect.column_type(definition, is_primary, auto_increment)
            }
        }
    }

    fn offset_without_limit(&self) -> String {
        match self {
            Self::Mysql => {
                let dialect = crate::MySqlDialect;
                dialect.offset_without_limit()
            }
            Self::Postgresql => {
                let dialect = crate::PostgresDialect;
                dialect.offset_without_limit()
            }
            Self::Sqlite => {
                let dialect = crate::SqliteDialect;
                dialect.offset_without_limit()
            }
        }
    }

    fn supports_native_enums(&self) -> bool {
        match self {
            Self::Mysql => {
                let dialect = crate::MySqlDialect;
                dialect.supports_native_enums()
            }
            Self::Postgresql => {
                let dialect = crate::PostgresDialect;
                dialect.supports_native_enums()
            }
            Self::Sqlite => {
                let dialect = crate::SqliteDialect;
                dialect.supports_native_enums()
            }
        }
    }

    fn supports_insert_returning(&self) -> bool {
        match self {
            Self::Mysql => {
                let dialect = crate::MySqlDialect;
                dialect.supports_insert_returning()
            }
            Self::Postgresql => {
                let dialect = crate::PostgresDialect;
                dialect.supports_insert_returning()
            }
            Self::Sqlite => {
                let dialect = crate::SqliteDialect;
                dialect.supports_insert_returning()
            }
        }
    }
}

#[async_trait]
impl DinocoAdapter for UniversalAdapter {
    type Dialect = UniversalDialect;

    fn dialect(&self) -> &Self::Dialect {
        match self {
            Self::Mysql(_) => &MYSQL_DIALECT,
            Self::Postgresql(_) => &POSTGRESQL_DIALECT,
            Self::Sqlite(_) => &SQLITE_DIALECT,
        }
    }

    async fn connect(url: String, config: DinocoClientConfig) -> DinocoResult<Self> {
        let Some(database) = detect_database(&url) else {
            return Err(DinocoError::ConnectionError(
                "Could not infer database type from URL. Use mysql://, postgresql://, postgres:// or file:."
                    .to_string(),
            ));
        };

        Self::connect_for_database(&database, url, config).await
    }

    async fn execute_result(&self, query: &str, params: &[DinocoValue]) -> DinocoResult<ExecutionResult> {
        match self {
            Self::Mysql(adapter) => adapter.execute_result(query, params).await,
            Self::Postgresql(adapter) => adapter.execute_result(query, params).await,
            Self::Sqlite(adapter) => adapter.execute_result(query, params).await,
        }
    }

    async fn execute_script(&self, sql_content: &str) -> DinocoResult<()> {
        match self {
            Self::Mysql(adapter) => adapter.execute_script(sql_content).await,
            Self::Postgresql(adapter) => adapter.execute_script(sql_content).await,
            Self::Sqlite(adapter) => adapter.execute_script(sql_content).await,
        }
    }

    async fn query_as<T: DinocoRow>(&self, query: &str, params: &[DinocoValue]) -> DinocoResult<Vec<T>> {
        match self {
            Self::Mysql(adapter) => adapter.query_as(query, params).await,
            Self::Postgresql(adapter) => adapter.query_as(query, params).await,
            Self::Sqlite(adapter) => adapter.query_as(query, params).await,
        }
    }
}

#[async_trait]
impl DinocoAdapterHandler for UniversalAdapter {
    async fn fetch_tables(&self) -> DinocoResult<Vec<DatabaseParsedTable>> {
        match self {
            Self::Mysql(adapter) => adapter.fetch_tables().await,
            Self::Postgresql(adapter) => adapter.fetch_tables().await,
            Self::Sqlite(adapter) => adapter.fetch_tables().await,
        }
    }

    async fn fetch_columns(&self, table_name: String) -> DinocoResult<Vec<DatabaseColumn>> {
        match self {
            Self::Mysql(adapter) => adapter.fetch_columns(table_name).await,
            Self::Postgresql(adapter) => adapter.fetch_columns(table_name).await,
            Self::Sqlite(adapter) => adapter.fetch_columns(table_name).await,
        }
    }

    async fn fetch_foreign_keys(&self) -> DinocoResult<Vec<DatabaseForeignKey>> {
        match self {
            Self::Mysql(adapter) => adapter.fetch_foreign_keys().await,
            Self::Postgresql(adapter) => adapter.fetch_foreign_keys().await,
            Self::Sqlite(adapter) => adapter.fetch_foreign_keys().await,
        }
    }

    async fn fetch_enums(&self) -> DinocoResult<Vec<DatabaseEnumRaw>> {
        match self {
            Self::Mysql(adapter) => adapter.fetch_enums().await,
            Self::Postgresql(adapter) => adapter.fetch_enums().await,
            Self::Sqlite(adapter) => adapter.fetch_enums().await,
        }
    }

    async fn fetch_indexes(&self) -> DinocoResult<Vec<DatabaseIndex>> {
        match self {
            Self::Mysql(adapter) => adapter.fetch_indexes().await,
            Self::Postgresql(adapter) => adapter.fetch_indexes().await,
            Self::Sqlite(adapter) => adapter.fetch_indexes().await,
        }
    }

    async fn reset_database(&self) -> DinocoResult<()> {
        match self {
            Self::Mysql(adapter) => adapter.reset_database().await,
            Self::Postgresql(adapter) => adapter.reset_database().await,
            Self::Sqlite(adapter) => adapter.reset_database().await,
        }
    }
}

impl MigrationExecutor for UniversalAdapter {
    fn build_step(&self, step: &MigrationStep, schema: &dinoco_compiler::ParsedSchema) -> Vec<String> {
        match self {
            Self::Mysql(adapter) => <MySqlAdapter as MigrationExecutor>::build_step(adapter, step, schema),
            Self::Postgresql(adapter) => <PostgresAdapter as MigrationExecutor>::build_step(adapter, step, schema),
            Self::Sqlite(adapter) => <SqliteAdapter as MigrationExecutor>::build_step(adapter, step, schema),
        }
    }

    fn build_reverse_step(&self, step: &MigrationStep, schema: &dinoco_compiler::ParsedSchema) -> Vec<String> {
        match self {
            Self::Mysql(adapter) => <MySqlAdapter as MigrationExecutor>::build_reverse_step(adapter, step, schema),
            Self::Postgresql(adapter) => {
                <PostgresAdapter as MigrationExecutor>::build_reverse_step(adapter, step, schema)
            }
            Self::Sqlite(adapter) => <SqliteAdapter as MigrationExecutor>::build_reverse_step(adapter, step, schema),
        }
    }

    fn build_migration(
        &self,
        steps: &[MigrationStep],
        schema: &dinoco_compiler::ParsedSchema,
        reverse: bool,
    ) -> Vec<String> {
        match self {
            Self::Mysql(adapter) => {
                <MySqlAdapter as MigrationExecutor>::build_migration(adapter, steps, schema, reverse)
            }
            Self::Postgresql(adapter) => {
                <PostgresAdapter as MigrationExecutor>::build_migration(adapter, steps, schema, reverse)
            }
            Self::Sqlite(adapter) => {
                <SqliteAdapter as MigrationExecutor>::build_migration(adapter, steps, schema, reverse)
            }
        }
    }
}

impl DinocoTransactionAdapter for UniversalAdapter {
    fn with_transaction<'a, T, F>(&'a self, operation: F) -> Pin<Box<dyn Future<Output = DinocoResult<T>> + Send + 'a>>
    where
        T: Send + 'a,
        F: FnOnce() -> Pin<Box<dyn Future<Output = DinocoResult<T>> + Send + 'a>> + Send + 'a,
    {
        match self {
            Self::Mysql(adapter) => adapter.with_transaction(operation),
            Self::Postgresql(adapter) => adapter.with_transaction(operation),
            Self::Sqlite(adapter) => adapter.with_transaction(operation),
        }
    }
}
