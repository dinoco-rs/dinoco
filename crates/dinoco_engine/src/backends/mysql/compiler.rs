use crate::backends::sql::{self, Dialect, escape_literal as escape_sql, referential_action};
use crate::{
    AddColumnMigration, AddForeignKeyMigration, AlterColumnMigration, AlterEnumMigration, CreateEnumMigration,
    CreateIndexMigration, CreateTableMigration, DinocoSqlCompiler, DropColumnMigration, DropEnumMigration,
    DropForeignKeyMigration, DropIndexMigration, DropTableMigration, MigrationColumn, MigrationColumnType,
    MigrationDefault, MigrationForeignKey, MigrationIndexKind, MySqlAdapter, RenameColumnMigration,
    RenameTableMigration,
};

const DIALECT: Dialect = Dialect::Mysql;

fn sql_identifier(identifier: &str) -> String {
    DIALECT.identifier(identifier)
}

fn quoted_identifier(identifier: &str) -> String {
    DIALECT.quoted_identifier(identifier)
}

impl DinocoSqlCompiler for MySqlAdapter {
    sql::query_compiler_methods!(DIALECT);

    fn compile_create_migrations_table(&self) -> String {
        "CREATE TABLE IF NOT EXISTS dinoco_migrations (name VARCHAR(255) PRIMARY KEY, applied_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP)"
            .to_string()
    }

    fn compile_insert_migration_record(&self, name: &str) -> String {
        format!("INSERT IGNORE INTO dinoco_migrations (name) VALUES ('{}')", escape_sql(name))
    }

    fn compile_create_table_migration(&self, migration: CreateTableMigration) -> String {
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
            .map(|column| compile_migration_column(column, inline_primary_key))
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

    fn compile_drop_table_migration(&self, migration: DropTableMigration) -> String {
        let if_exists = if migration.if_exists { " IF EXISTS" } else { "" };
        format!("DROP TABLE{if_exists} {};", sql_identifier(&migration.table))
    }

    fn compile_rename_table_migration(&self, migration: RenameTableMigration) -> Vec<String> {
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
            compile_migration_column(&migration.column, true)
        )
    }

    fn compile_drop_column_migration(&self, migration: DropColumnMigration) -> String {
        format!("ALTER TABLE {} DROP COLUMN {};", sql_identifier(&migration.table), sql_identifier(&migration.column))
    }

    fn compile_alter_column_migration(&self, migration: AlterColumnMigration) -> Vec<String> {
        vec![format!(
            "ALTER TABLE {} MODIFY COLUMN {};",
            sql_identifier(&migration.table),
            compile_migration_column(&migration.desired, true)
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
            "ALTER TABLE {} ADD {};",
            sql_identifier(&migration.table),
            compile_foreign_key(&migration.foreign_key)
        )]
    }

    fn compile_drop_foreign_key_migration(&self, migration: DropForeignKeyMigration) -> Vec<String> {
        vec![format!(
            "ALTER TABLE {} DROP FOREIGN KEY {};",
            sql_identifier(&migration.table),
            quoted_identifier(&migration.name)
        )]
    }

    fn compile_create_index_migration(&self, migration: CreateIndexMigration) -> String {
        let index_type = match migration.index.kind {
            MigrationIndexKind::Standard => "",
            MigrationIndexKind::Unique => "UNIQUE ",
            MigrationIndexKind::FullText => "FULLTEXT ",
        };
        let columns = migration.index.columns.iter().map(|column| sql_identifier(column)).collect::<Vec<_>>();
        let Some(predicate) = &migration.index.predicate else {
            return format!(
                "CREATE {index_type}INDEX {} ON {} ({});",
                sql_identifier(&migration.index.name),
                sql_identifier(&migration.table),
                columns.join(", ")
            );
        };

        // MySQL has no partial indexes. A unique one keys every column by
        // `CASE WHEN <condition> THEN column END`, which is NULL outside the
        // condition, and NULLs never collide. Any other index covers every
        // row, which serves the same queries. Either way the condition is kept
        // in the index comment for drift detection.
        let key_parts = if migration.index.kind == MigrationIndexKind::Unique {
            let condition = predicate.to_sql(&|field| sql_identifier(field), true);
            columns.iter().map(|column| format!("(CASE WHEN {condition} THEN {column} END)")).collect::<Vec<_>>()
        } else {
            columns
        };
        format!(
            "CREATE {index_type}INDEX {} ON {} ({}) COMMENT '{}';",
            sql_identifier(&migration.index.name),
            sql_identifier(&migration.table),
            key_parts.join(", "),
            predicate.comment().replace('\\', "\\\\").replace('\'', "''")
        )
    }

    fn compile_drop_index_migration(&self, migration: DropIndexMigration) -> String {
        format!("DROP INDEX {} ON {};", sql_identifier(&migration.index.name), sql_identifier(&migration.table))
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

fn compile_migration_column(column: &MigrationColumn, inline_primary_key: bool) -> String {
    let mut parts = vec![sql_identifier(&column.name), migration_type(&column.ty)];

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
        && let Some(default) = migration_default(default)
    {
        parts.push(default);
    }

    parts.join(" ")
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

fn migration_type(ty: &MigrationColumnType) -> String {
    match ty {
        MigrationColumnType::String | MigrationColumnType::Text => "VARCHAR(255)".to_string(),
        MigrationColumnType::Boolean => "TINYINT(1)".to_string(),
        MigrationColumnType::Integer => "BIGINT".to_string(),
        MigrationColumnType::Float => "DOUBLE PRECISION".to_string(),
        MigrationColumnType::DateTime => "TIMESTAMP".to_string(),
        MigrationColumnType::Date => "DATE".to_string(),
        MigrationColumnType::Json => "JSON".to_string(),
        MigrationColumnType::Enum { values, .. } => {
            let values = values.iter().map(|value| format!("'{}'", escape_sql(value))).collect::<Vec<_>>().join(", ");
            format!("ENUM({values})")
        }
    }
}

fn migration_default(default: &MigrationDefault) -> Option<String> {
    match default {
        MigrationDefault::String(value) => Some(format!("DEFAULT '{}'", escape_sql(value))),
        MigrationDefault::Boolean(value) => Some(format!("DEFAULT {value}")),
        MigrationDefault::Integer(value) => Some(format!("DEFAULT {value}")),
        MigrationDefault::Float(value) => Some(format!("DEFAULT {value}")),
        MigrationDefault::CurrentTimestamp => Some("DEFAULT CURRENT_TIMESTAMP".to_string()),
        MigrationDefault::AutoIncrement => Some("AUTO_INCREMENT".to_string()),
    }
}
