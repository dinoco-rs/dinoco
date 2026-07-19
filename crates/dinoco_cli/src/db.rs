use anyhow::Context;
use dinoco_engine::{
    AddColumnMigration, AddForeignKeyMigration, AlterColumnMigration, AlterEnumMigration, CreateEnumMigration,
    CreateTableMigration, DinocoAdapter, DinocoSqlCompiler, DropColumnMigration, DropEnumMigration,
    DropForeignKeyMigration, DropTableMigration, MigrationColumn, MigrationColumnType, MigrationForeignKey,
    MySqlAdapter, PgBouncerAdapter, PostgresAdapter, ReferentialAction, RenameColumnMigration, SqliteAdapter,
};
use dinoco_engine::{mysql_async::prelude::Queryable, rusqlite};

use crate::schema::{Database, PostgresConnection, RuntimeConfig};

pub enum CliDatabase {
    Postgres(PostgresAdapter),
    PgBouncer(PgBouncerAdapter),
    Mysql(MySqlAdapter),
    Sqlite(SqliteAdapter),
}

#[derive(Debug, Clone, Default)]
pub struct DatabaseSchema {
    pub tables: Vec<DatabaseTable>,
    pub enums: Vec<DatabaseEnum>,
}

#[derive(Debug, Clone)]
pub struct DatabaseTable {
    pub name: String,
    pub row_count: i64,
    pub columns: Vec<MigrationColumn>,
    pub foreign_keys: Vec<MigrationForeignKey>,
}

#[derive(Debug, Clone)]
pub struct DatabaseEnum {
    pub name: String,
    pub values: Vec<String>,
}

impl CliDatabase {
    pub async fn connect(config: &RuntimeConfig) -> anyhow::Result<Self> {
        match (config.database, config.postgres_connection) {
            (Database::Postgresql, PostgresConnection::Direct) => {
                Ok(Self::Postgres(PostgresAdapter::direct(&config.database_url).await?))
            }
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
                "SELECT COUNT(*) FROM information_schema.tables WHERE table_schema = 'public' AND table_type = 'BASE TABLE' AND table_name != 'dinoco_migrations'"
            }
            Self::Mysql(_) => {
                "SELECT COUNT(*) FROM information_schema.tables WHERE table_schema = DATABASE() AND table_type = 'BASE TABLE' AND table_name != 'dinoco_migrations'"
            }
            Self::Sqlite(_) => {
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name != 'dinoco_migrations'"
            }
        };

        Ok(self.count(sql).await.context("failed to inspect database tables")? > 0)
    }

    pub async fn migration_applied(&self, name: &str) -> anyhow::Result<bool> {
        let name = name.replace('\'', "''");
        let sql = format!("SELECT COUNT(*) FROM dinoco_migrations WHERE name = '{name}'");

        Ok(self.count(&sql).await? > 0)
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

async fn inspect_sqlite(adapter: &SqliteAdapter) -> anyhow::Result<DatabaseSchema> {
    let conn = adapter.pool.get().await.context("failed to get sqlite connection from pool")?;

    conn.interact(move |conn| -> anyhow::Result<DatabaseSchema> {
        let mut tables_stmt = conn.prepare(
            "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' AND name != 'dinoco_migrations' ORDER BY name",
        )?;
        let table_names = tables_stmt
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        let mut tables = Vec::new();

        for table_name in table_names {
            let row_count =
                conn.query_row(&format!("SELECT COUNT(*) FROM {table_name}"), [], |row| row.get::<_, i64>(0))?;
            let mut columns_stmt = conn.prepare(&format!("PRAGMA table_info({table_name})"))?;
            let columns = columns_stmt
                .query_map([], |row| sqlite_column(row))?
                .collect::<Result<Vec<_>, _>>()?;
            let foreign_keys = sqlite_foreign_keys(conn, &table_name)?;

            tables.push(DatabaseTable { name: table_name, row_count, columns, foreign_keys });
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
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;

    let mut grouped: Vec<MigrationForeignKey> = Vec::new();
    for (id, references_table, column, references_column, on_update, on_delete) in rows {
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

    Ok(MigrationColumn {
        name,
        ty: parse_column_type(&raw_type),
        primary_key: primary_key > 0,
        nullable: not_null == 0 && primary_key == 0,
        default: default.and_then(|value| parse_default(&value)),
    })
}

async fn inspect_postgres(adapter: &PostgresAdapter) -> anyhow::Result<DatabaseSchema> {
    let conn = adapter.pool.get().await.context("failed to get postgres connection from pool")?;
    let table_rows = conn
        .query(
            "SELECT table_name FROM information_schema.tables WHERE table_schema = 'public' AND table_type = 'BASE TABLE' AND table_name != 'dinoco_migrations' ORDER BY table_name",
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
                "SELECT c.column_name, c.data_type, c.is_nullable, c.column_default, CASE WHEN kcu.column_name IS NULL THEN false ELSE true END AS primary_key, c.udt_name
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
                Ok(MigrationColumn {
                    name: row.try_get(0)?,
                    ty: if raw_type == "USER-DEFINED" {
                        MigrationColumnType::Enum { name: udt_name, values: Vec::new() }
                    } else {
                        parse_column_type(&raw_type)
                    },
                    primary_key: row.try_get(4)?,
                    nullable: nullable == "YES",
                    default: default.and_then(|value| parse_default(&value)),
                })
            })
            .collect::<anyhow::Result<Vec<_>>>()?;
        let foreign_keys = postgres_foreign_keys(&conn, &table_name).await?;

        tables.push(DatabaseTable { name: table_name, row_count, columns, foreign_keys });
    }

    let enum_rows = conn
        .query(
            "SELECT t.typname, e.enumlabel FROM pg_type t JOIN pg_enum e ON e.enumtypid = t.oid JOIN pg_namespace n ON n.oid = t.typnamespace WHERE n.nspname = 'public' ORDER BY t.typname, e.enumsortorder",
            &[],
        )
        .await?;
    let enums = fold_enum_rows(enum_rows.into_iter().map(|row| Ok((row.try_get(0)?, row.try_get(1)?))))?;

    Ok(DatabaseSchema { tables, enums })
}

async fn postgres_foreign_keys(
    conn: &dinoco_engine::deadpool_postgres::Client,
    table_name: &str,
) -> anyhow::Result<Vec<MigrationForeignKey>> {
    let rows = conn
        .query(
            "SELECT tc.constraint_name, kcu.column_name, ccu.table_name AS foreign_table_name, ccu.column_name AS foreign_column_name, rc.update_rule, rc.delete_rule
             FROM information_schema.table_constraints tc
             JOIN information_schema.key_column_usage kcu ON kcu.constraint_schema = tc.constraint_schema AND kcu.constraint_name = tc.constraint_name AND kcu.table_name = tc.table_name
             JOIN information_schema.constraint_column_usage ccu ON ccu.constraint_schema = tc.constraint_schema AND ccu.constraint_name = tc.constraint_name
             JOIN information_schema.referential_constraints rc ON rc.constraint_schema = tc.constraint_schema AND rc.constraint_name = tc.constraint_name
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

async fn inspect_pgbouncer(adapter: &PgBouncerAdapter) -> anyhow::Result<DatabaseSchema> {
    inspect_postgres(adapter.inner()).await
}

async fn inspect_mysql(adapter: &MySqlAdapter) -> anyhow::Result<DatabaseSchema> {
    let mut conn = adapter.pool.get_conn().await.context("failed to get mysql connection from pool")?;
    let table_names: Vec<String> = conn
        .query(
            "SELECT table_name FROM information_schema.tables WHERE table_schema = DATABASE() AND table_type = 'BASE TABLE' AND table_name != 'dinoco_migrations' ORDER BY table_name",
        )
        .await?;
    let mut tables = Vec::new();

    for table_name in table_names {
        let row_count: Option<i64> = conn.query_first(format!("SELECT COUNT(*) FROM {table_name}")).await?;
        let rows: Vec<dinoco_engine::mysql_async::Row> = conn
            .exec(
                "SELECT column_name AS name, data_type AS raw_type, column_type AS column_type, is_nullable AS nullable, column_default AS default_value, column_key AS column_key FROM information_schema.columns WHERE table_schema = DATABASE() AND table_name = ? ORDER BY ordinal_position",
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
                MigrationColumn {
                    name: row.take::<String, _>("name").unwrap_or_default(),
                    ty: if raw_type == "enum" {
                        MigrationColumnType::Enum { name: String::new(), values: parse_mysql_enum_values(&column_type) }
                    } else {
                        parse_column_type(&raw_type)
                    },
                    primary_key: key == "PRI",
                    nullable: nullable == "YES",
                    default: row
                        .take::<Option<String>, _>("default_value")
                        .flatten()
                        .and_then(|value| parse_default(&value)),
                }
            })
            .collect();
        let foreign_keys = mysql_foreign_keys(&mut conn, &table_name).await?;

        tables.push(DatabaseTable {
            name: table_name,
            row_count: row_count.unwrap_or_default(),
            columns,
            foreign_keys,
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

fn parse_column_type(raw: &str) -> MigrationColumnType {
    let raw = raw.to_ascii_lowercase();
    if raw.contains("json") {
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

fn parse_default(raw: &str) -> Option<dinoco_engine::MigrationDefault> {
    let value = raw.trim().trim_matches('\'').trim_matches('"');
    if value.eq_ignore_ascii_case("null") {
        None
    } else if value.eq_ignore_ascii_case("true") || value == "1" {
        Some(dinoco_engine::MigrationDefault::Boolean(true))
    } else if value.eq_ignore_ascii_case("false") || value == "0" {
        Some(dinoco_engine::MigrationDefault::Boolean(false))
    } else if value.to_ascii_lowercase().contains("current_timestamp") || value.to_ascii_lowercase().contains("now()") {
        Some(dinoco_engine::MigrationDefault::CurrentTimestamp)
    } else {
        None
    }
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
