use crate::backends::sql::{self, Dialect, escape_literal as escape_sql, referential_action};
use crate::{
    AddColumnMigration, AddForeignKeyMigration, AlterColumnMigration, AlterEnumMigration, CreateEnumMigration,
    CreateIndexMigration, CreateTableMigration, DinocoSqlCompiler, DropColumnMigration, DropEnumMigration,
    DropForeignKeyMigration, DropIndexMigration, DropTableMigration, MigrationColumn, MigrationColumnType,
    MigrationDefault, MigrationForeignKey, MigrationIndexKind, RenameColumnMigration, RenameTableMigration,
    SqliteAdapter,
};

const DIALECT: Dialect = Dialect::Sqlite;

fn sql_identifier(identifier: &str) -> String {
    DIALECT.identifier(identifier)
}

fn quoted_identifier(identifier: &str) -> String {
    DIALECT.quoted_identifier(identifier)
}

impl DinocoSqlCompiler for SqliteAdapter {
    sql::query_compiler_methods!(DIALECT);

    fn compile_create_migrations_table(&self) -> String {
        "CREATE TABLE IF NOT EXISTS dinoco_migrations (name TEXT PRIMARY KEY, applied_at TEXT DEFAULT CURRENT_TIMESTAMP)"
            .to_string()
    }

    fn compile_insert_migration_record(&self, name: &str) -> String {
        format!("INSERT OR IGNORE INTO dinoco_migrations (name) VALUES ('{}')", escape_sql(name))
    }

    fn compile_create_table_migration(&self, migration: CreateTableMigration) -> String {
        compile_create_table_migration(migration, DatabaseDialect::Sqlite)
    }

    fn compile_drop_table_migration(&self, migration: DropTableMigration) -> String {
        let if_exists = if migration.if_exists { " IF EXISTS" } else { "" };
        format!("DROP TABLE{if_exists} {};", sql_identifier(&migration.table))
    }

    fn compile_rename_table_migration(&self, migration: RenameTableMigration) -> Vec<String> {
        if migration.from != migration.to && migration.from.eq_ignore_ascii_case(&migration.to) {
            let temporary = format!("__dinoco_legacy_rename_{}", migration.to);
            return vec![
                format!(
                    "ALTER TABLE {} RENAME TO {};",
                    quoted_identifier(&migration.from),
                    quoted_identifier(&temporary)
                ),
                format!(
                    "ALTER TABLE {} RENAME TO {};",
                    quoted_identifier(&temporary),
                    quoted_identifier(&migration.to)
                ),
            ];
        }
        vec![format!(
            "ALTER TABLE {} RENAME TO {};",
            quoted_identifier(&migration.from),
            quoted_identifier(&migration.to)
        )]
    }

    fn compile_add_column_migration(&self, migration: AddColumnMigration) -> String {
        format!(
            "ALTER TABLE {} ADD COLUMN {};",
            sql_identifier(&migration.table),
            compile_migration_column(&migration.column, DatabaseDialect::Sqlite, true)
        )
    }

    fn compile_drop_column_migration(&self, migration: DropColumnMigration) -> String {
        format!("ALTER TABLE {} DROP COLUMN {};", sql_identifier(&migration.table), sql_identifier(&migration.column))
    }

    fn compile_alter_column_migration(&self, migration: AlterColumnMigration) -> Vec<String> {
        vec![format!(
            "-- SQLite cannot safely alter column `{}` on `{}` in place. Rebuild the table manually or create a custom migration.",
            migration.desired.name, migration.table
        )]
    }

    fn compile_rename_column_migration(&self, migration: RenameColumnMigration) -> Vec<String> {
        vec![format!(
            "ALTER TABLE {} RENAME COLUMN {} TO {};",
            sql_identifier(&migration.table),
            quoted_identifier(&migration.from),
            quoted_identifier(&migration.to)
        )]
    }

    fn compile_add_foreign_key_migration(&self, migration: AddForeignKeyMigration) -> Vec<String> {
        vec![format!(
            "-- SQLite cannot add foreign key `{}` on `{}` after table creation. Rebuild the table manually or create a custom migration.",
            migration.foreign_key.name, migration.table
        )]
    }

    fn compile_drop_foreign_key_migration(&self, migration: DropForeignKeyMigration) -> Vec<String> {
        vec![format!(
            "-- SQLite cannot drop foreign key `{}` on `{}` in place. Rebuild the table manually or create a custom migration.",
            migration.name, migration.table
        )]
    }

    fn compile_create_index_migration(&self, migration: CreateIndexMigration) -> String {
        if migration.index.kind == MigrationIndexKind::FullText {
            return format!(
                "-- SQLite does not support native full-text indexes for `{}.{}`; Dinoco uses LIKE fallback queries.",
                migration.table,
                migration.index.columns.join(", ")
            );
        }
        let unique = if migration.index.kind == MigrationIndexKind::Unique { "UNIQUE " } else { "" };
        // The condition is kept as a comment too: SQLite stores the statement's
        // text, which is where drift detection reads it back from.
        let predicate = migration.index.predicate.as_ref().map_or_else(String::new, |predicate| {
            format!(" WHERE {} /* {} */", predicate.to_sql(&|field| sql_identifier(field), false), predicate.comment())
        });
        format!(
            "CREATE {unique}INDEX {} ON {} ({}){predicate};",
            sql_identifier(&migration.index.name),
            sql_identifier(&migration.table),
            migration.index.columns.iter().map(|column| sql_identifier(column)).collect::<Vec<_>>().join(", ")
        )
    }

    fn compile_drop_index_migration(&self, migration: DropIndexMigration) -> String {
        format!("DROP INDEX {};", sql_identifier(&migration.index.name))
    }

    fn compile_create_enum_migration(&self, _migration: CreateEnumMigration) -> Vec<String> {
        Vec::new()
    }

    fn compile_drop_enum_migration(&self, _migration: DropEnumMigration) -> Vec<String> {
        Vec::new()
    }

    fn compile_alter_enum_migration(&self, _migration: AlterEnumMigration) -> Vec<String> {
        Vec::new()
    }
}

#[derive(Clone, Copy)]
enum DatabaseDialect {
    Sqlite,
}

fn compile_create_table_migration(migration: CreateTableMigration, dialect: DatabaseDialect) -> String {
    let if_not_exists = if migration.if_not_exists { " IF NOT EXISTS" } else { "" };
    let primary_key_columns = migration
        .columns
        .iter()
        .filter(|column| column.primary_key)
        .map(|column| sql_identifier(&column.name))
        .collect::<Vec<_>>();
    let inline_primary_key = primary_key_columns.len() <= 1;
    let mut definitions = migration
        .columns
        .iter()
        .map(|column| compile_migration_column(column, dialect, inline_primary_key))
        .collect::<Vec<_>>();
    if !inline_primary_key {
        definitions.push(format!("PRIMARY KEY ({})", primary_key_columns.join(", ")));
    }
    definitions.extend(migration.foreign_keys.iter().map(compile_foreign_key));

    format!(
        "CREATE TABLE{if_not_exists} {} (\n    {}\n);",
        sql_identifier(&migration.table),
        definitions.join(",\n    ")
    )
}

fn compile_foreign_key(foreign_key: &MigrationForeignKey) -> String {
    format!(
        "CONSTRAINT {} FOREIGN KEY ({}) REFERENCES {} ({}) ON UPDATE {} ON DELETE {}",
        sql_identifier(&foreign_key.name),
        foreign_key.columns.iter().map(|column| sql_identifier(column)).collect::<Vec<_>>().join(", "),
        sql_identifier(&foreign_key.references_table),
        foreign_key.references_columns.iter().map(|column| sql_identifier(column)).collect::<Vec<_>>().join(", "),
        referential_action(foreign_key.on_update),
        referential_action(foreign_key.on_delete),
    )
}

fn compile_migration_column(column: &MigrationColumn, dialect: DatabaseDialect, inline_primary_key: bool) -> String {
    let mut parts = vec![sql_identifier(&column.name), migration_type(&column.ty, dialect).to_string()];

    if let MigrationColumnType::Enum { values, .. } = &column.ty {
        let values = values.iter().map(|value| format!("'{}'", escape_sql(value))).collect::<Vec<_>>().join(", ");
        parts.push(format!("CHECK ({} IN ({values}))", sql_identifier(&column.name)));
    }

    if column.primary_key && inline_primary_key {
        parts.push("PRIMARY KEY".to_string());
    }

    if column.unique && !column.primary_key {
        parts.push("UNIQUE".to_string());
    }

    if !column.nullable {
        parts.push("NOT NULL".to_string());
    }

    if let Some(default) = &column.default
        && let Some(default) = migration_default(default, dialect)
    {
        parts.push(default);
    }

    parts.join(" ")
}

fn migration_type(ty: &MigrationColumnType, _dialect: DatabaseDialect) -> &'static str {
    match ty {
        MigrationColumnType::String | MigrationColumnType::Text => "TEXT",
        MigrationColumnType::Boolean => "BOOLEAN",
        MigrationColumnType::Integer => "INTEGER",
        MigrationColumnType::Float => "REAL",
        MigrationColumnType::DateTime => "DATETIME",
        MigrationColumnType::Date => "DATE",
        MigrationColumnType::Json => "BLOB",
        MigrationColumnType::Enum { .. } => "TEXT",
    }
}

fn migration_default(default: &MigrationDefault, _dialect: DatabaseDialect) -> Option<String> {
    match default {
        MigrationDefault::String(value) => Some(format!("DEFAULT '{}'", escape_sql(value))),
        MigrationDefault::Boolean(value) => Some(format!("DEFAULT {}", if *value { 1 } else { 0 })),
        MigrationDefault::Integer(value) => Some(format!("DEFAULT {value}")),
        MigrationDefault::Float(value) => Some(format!("DEFAULT {value}")),
        MigrationDefault::CurrentTimestamp => Some("DEFAULT CURRENT_TIMESTAMP".to_string()),
        MigrationDefault::AutoIncrement => None,
    }
}
