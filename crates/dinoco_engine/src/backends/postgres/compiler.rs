use crate::backends::sql::{self, Dialect, escape_literal as escape_sql, referential_action};
use crate::{
    AddColumnMigration, AddForeignKeyMigration, AlterColumnMigration, AlterEnumMigration, CreateEnumMigration,
    CreateIndexMigration, CreateTableMigration, DinocoSqlCompiler, DropColumnMigration, DropEnumMigration,
    DropForeignKeyMigration, DropIndexMigration, DropTableMigration, MigrationColumn, MigrationColumnType,
    MigrationDefault, MigrationForeignKey, MigrationIndexKind, RenameColumnMigration, RenameTableMigration,
};

use super::{PgBouncerAdapter, PostgresAdapter};

const DIALECT: Dialect = Dialect::Postgres;

fn sql_identifier(identifier: &str) -> String {
    DIALECT.identifier(identifier)
}

fn quoted_identifier(identifier: &str) -> String {
    DIALECT.quoted_identifier(identifier)
}

/// PgBouncer speaks the same SQL; only how statements are prepared differs.
macro_rules! postgres_compiler {
    ($($adapter:ty),*) => {
        $(
            impl DinocoSqlCompiler for $adapter {
                sql::query_compiler_methods!(DIALECT);

                fn compile_create_migrations_table(&self) -> String {
                    compile_create_migrations_table()
                }

                fn compile_insert_migration_record(&self, name: &str) -> String {
                    compile_insert_migration_record(name)
                }

                fn compile_create_table_migration(&self, migration: CreateTableMigration) -> String {
                    compile_create_table_migration(migration)
                }

                fn compile_drop_table_migration(&self, migration: DropTableMigration) -> String {
                    compile_drop_table_migration(migration)
                }

                fn compile_rename_table_migration(&self, migration: RenameTableMigration) -> Vec<String> {
                    compile_rename_table_migration(migration)
                }

                fn compile_add_column_migration(&self, migration: AddColumnMigration) -> String {
                    compile_add_column_migration(migration)
                }

                fn compile_drop_column_migration(&self, migration: DropColumnMigration) -> String {
                    compile_drop_column_migration(migration)
                }

                fn compile_alter_column_migration(&self, migration: AlterColumnMigration) -> Vec<String> {
                    compile_alter_column_migration(migration)
                }

                fn compile_rename_column_migration(&self, migration: RenameColumnMigration) -> Vec<String> {
                    compile_rename_column_migration(migration)
                }

                fn compile_add_foreign_key_migration(&self, migration: AddForeignKeyMigration) -> Vec<String> {
                    compile_add_foreign_key_migration(migration)
                }

                fn compile_add_unvalidated_foreign_key_migration(&self, migration: AddForeignKeyMigration) -> Vec<String> {
                    compile_add_unvalidated_foreign_key_migration(migration)
                }

                fn compile_drop_foreign_key_migration(&self, migration: DropForeignKeyMigration) -> Vec<String> {
                    compile_drop_foreign_key_migration(migration)
                }

                fn compile_create_index_migration(&self, migration: CreateIndexMigration) -> String {
                    compile_create_index_migration(migration)
                }

                fn compile_drop_index_migration(&self, migration: DropIndexMigration) -> String {
                    compile_drop_index_migration(migration)
                }

                fn compile_create_enum_migration(&self, migration: CreateEnumMigration) -> Vec<String> {
                    compile_create_enum_migration(migration)
                }

                fn compile_drop_enum_migration(&self, migration: DropEnumMigration) -> Vec<String> {
                    compile_drop_enum_migration(migration)
                }

                fn compile_alter_enum_migration(&self, migration: AlterEnumMigration) -> Vec<String> {
                    compile_alter_enum_migration(migration)
                }
            }
        )*
    };
}

postgres_compiler!(PostgresAdapter, PgBouncerAdapter);

fn compile_create_migrations_table() -> String {
    "CREATE TABLE IF NOT EXISTS dinoco_migrations (name VARCHAR(255) PRIMARY KEY, applied_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP)"
        .to_string()
}

fn compile_insert_migration_record(name: &str) -> String {
    format!("INSERT INTO dinoco_migrations (name) VALUES ('{}') ON CONFLICT (name) DO NOTHING", escape_sql(name))
}

fn compile_create_table_migration(migration: CreateTableMigration) -> String {
    let if_not_exists = if migration.if_not_exists { " IF NOT EXISTS" } else { "" };
    let primary_key_columns = migration
        .columns
        .iter()
        .filter(|column| column.primary_key)
        .map(|column| sql_identifier(&column.name))
        .collect::<Vec<_>>();
    let inline_primary_key = primary_key_columns.len() <= 1;
    let mut definitions =
        migration.columns.iter().map(|column| compile_migration_column(column, inline_primary_key)).collect::<Vec<_>>();
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

fn compile_drop_table_migration(migration: DropTableMigration) -> String {
    let if_exists = if migration.if_exists { " IF EXISTS" } else { "" };
    format!("DROP TABLE{if_exists} {};", sql_identifier(&migration.table))
}

fn compile_rename_table_migration(migration: RenameTableMigration) -> Vec<String> {
    vec![format!("ALTER TABLE {} RENAME TO {};", quoted_identifier(&migration.from), quoted_identifier(&migration.to))]
}

fn compile_add_column_migration(migration: AddColumnMigration) -> String {
    format!(
        "ALTER TABLE {} ADD COLUMN {};",
        sql_identifier(&migration.table),
        compile_migration_column(&migration.column, true)
    )
}

fn compile_drop_column_migration(migration: DropColumnMigration) -> String {
    format!("ALTER TABLE {} DROP COLUMN {};", sql_identifier(&migration.table), sql_identifier(&migration.column))
}

fn compile_alter_column_migration(migration: AlterColumnMigration) -> Vec<String> {
    let mut statements = Vec::new();
    let table = sql_identifier(&migration.table);
    let current = migration.current;
    let desired = migration.desired;

    if current.ty != desired.ty {
        statements.push(format!(
            "ALTER TABLE {table} ALTER COLUMN {} TYPE {};",
            sql_identifier(&desired.name),
            migration_type(&desired.ty)
        ));
    }

    if current.nullable != desired.nullable {
        let action = if desired.nullable { "DROP NOT NULL" } else { "SET NOT NULL" };
        statements.push(format!("ALTER TABLE {table} ALTER COLUMN {} {action};", sql_identifier(&desired.name)));
    }

    if current.default != desired.default {
        match &desired.default {
            Some(default) => {
                if let Some(default) = migration_default(default) {
                    statements.push(format!(
                        "ALTER TABLE {table} ALTER COLUMN {} SET {default};",
                        sql_identifier(&desired.name)
                    ));
                }
            }
            None => statements
                .push(format!("ALTER TABLE {table} ALTER COLUMN {} DROP DEFAULT;", sql_identifier(&desired.name))),
        }
    }

    statements
}

fn compile_rename_column_migration(migration: RenameColumnMigration) -> Vec<String> {
    vec![format!(
        "ALTER TABLE {} RENAME COLUMN {} TO {};",
        sql_identifier(&migration.table),
        quoted_identifier(&migration.from),
        quoted_identifier(&migration.to)
    )]
}

fn compile_add_foreign_key_migration(migration: AddForeignKeyMigration) -> Vec<String> {
    vec![format!(
        "ALTER TABLE {} ADD {};",
        sql_identifier(&migration.table),
        compile_foreign_key(&migration.foreign_key)
    )]
}

fn compile_add_unvalidated_foreign_key_migration(migration: AddForeignKeyMigration) -> Vec<String> {
    vec![format!(
        "ALTER TABLE {} ADD {} NOT VALID;",
        sql_identifier(&migration.table),
        compile_foreign_key(&migration.foreign_key)
    )]
}

fn compile_drop_foreign_key_migration(migration: DropForeignKeyMigration) -> Vec<String> {
    vec![format!(
        "ALTER TABLE {} DROP CONSTRAINT IF EXISTS {};",
        sql_identifier(&migration.table),
        quoted_identifier(&migration.name)
    )]
}

fn compile_create_index_migration(migration: CreateIndexMigration) -> String {
    if migration.index.kind == MigrationIndexKind::FullText {
        let document = migration
            .index
            .columns
            .iter()
            .map(|column| format!("COALESCE({}, '')", sql_identifier(column)))
            .collect::<Vec<_>>()
            .join(" || ' ' || ");
        return format!(
            "CREATE INDEX {} ON {} USING GIN (to_tsvector('simple', {document}));",
            sql_identifier(&migration.index.name),
            sql_identifier(&migration.table),
        );
    }
    let unique = if migration.index.kind == MigrationIndexKind::Unique { "UNIQUE " } else { "" };
    let create = format!(
        "CREATE {unique}INDEX {} ON {} ({})",
        sql_identifier(&migration.index.name),
        sql_identifier(&migration.table),
        migration.index.columns.iter().map(|column| sql_identifier(column)).collect::<Vec<_>>().join(", ")
    );
    let Some(predicate) = &migration.index.predicate else {
        return format!("{create};");
    };
    // `pg_get_expr` rewrites a condition with casts and parentheses, so drift
    // detection reads Dinoco's own text back from the index comment instead.
    format!(
        "{create} WHERE {};\nCOMMENT ON INDEX {} IS '{}';",
        predicate.to_sql(&|field| sql_identifier(field), false),
        sql_identifier(&migration.index.name),
        predicate.comment().replace('\'', "''")
    )
}

fn compile_drop_index_migration(migration: DropIndexMigration) -> String {
    format!("DROP INDEX {};", sql_identifier(&migration.index.name))
}

fn compile_foreign_key(foreign_key: &MigrationForeignKey) -> String {
    format!(
        "CONSTRAINT {} FOREIGN KEY ({}) REFERENCES {} ({}) ON UPDATE {} ON DELETE {}",
        quoted_identifier(&foreign_key.name),
        foreign_key.columns.iter().map(|column| sql_identifier(column)).collect::<Vec<_>>().join(", "),
        sql_identifier(&foreign_key.references_table),
        foreign_key.references_columns.iter().map(|column| sql_identifier(column)).collect::<Vec<_>>().join(", "),
        referential_action(foreign_key.on_update),
        referential_action(foreign_key.on_delete),
    )
}

fn compile_create_enum_migration(migration: CreateEnumMigration) -> Vec<String> {
    let values = migration.values.iter().map(|value| format!("'{}'", escape_sql(value))).collect::<Vec<_>>().join(", ");
    vec![format!("CREATE TYPE {} AS ENUM ({values});", quoted_identifier(&migration.name))]
}

fn compile_drop_enum_migration(migration: DropEnumMigration) -> Vec<String> {
    vec![format!("DROP TYPE IF EXISTS {};", quoted_identifier(&migration.name))]
}

fn compile_alter_enum_migration(migration: AlterEnumMigration) -> Vec<String> {
    migration
        .desired_values
        .iter()
        .filter(|value| !migration.current_values.contains(value))
        .map(|value| {
            format!(
                "ALTER TYPE {} ADD VALUE IF NOT EXISTS '{}';",
                quoted_identifier(&migration.name),
                escape_sql(value)
            )
        })
        .collect()
}

fn compile_migration_column(column: &MigrationColumn, inline_primary_key: bool) -> String {
    let mut parts = vec![sql_identifier(&column.name), migration_type(&column.ty).to_string()];

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

fn migration_type(ty: &MigrationColumnType) -> String {
    match ty {
        MigrationColumnType::String | MigrationColumnType::Text => "TEXT".to_string(),
        MigrationColumnType::Boolean => "BOOLEAN".to_string(),
        MigrationColumnType::Integer => "BIGINT".to_string(),
        MigrationColumnType::Float => "DOUBLE PRECISION".to_string(),
        MigrationColumnType::DateTime => "TIMESTAMP".to_string(),
        MigrationColumnType::Date => "DATE".to_string(),
        MigrationColumnType::Json => "JSONB".to_string(),
        MigrationColumnType::Enum { name, .. } => quoted_identifier(name),
    }
}

fn migration_default(default: &MigrationDefault) -> Option<String> {
    match default {
        MigrationDefault::String(value) => Some(format!("DEFAULT '{}'", escape_sql(value))),
        MigrationDefault::Boolean(value) => Some(format!("DEFAULT {value}")),
        MigrationDefault::Integer(value) => Some(format!("DEFAULT {value}")),
        MigrationDefault::Float(value) => Some(format!("DEFAULT {value}")),
        MigrationDefault::CurrentTimestamp => Some("DEFAULT CURRENT_TIMESTAMP".to_string()),
        MigrationDefault::AutoIncrement => Some("GENERATED BY DEFAULT AS IDENTITY".to_string()),
    }
}
