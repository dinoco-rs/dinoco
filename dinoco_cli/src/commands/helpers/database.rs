use dinoco_compiler::Database;
use dinoco_engine::{ColumnDefault, ColumnDefinition, ColumnType, CreateTableStatement, DinocoAdapter, DinocoResult, DinocoValue, DropTableStatement};

use crate::{DatabaseColumn, DatabaseForeignKey, DatabaseParsedTable, DatabaseTable, DinocoMigration};

pub async fn drop_all_tables<T: DinocoAdapter>(adapter: &T, tables: Vec<DatabaseParsedTable>) -> DinocoResult<()> {
    let dialect = adapter.dialect();

    for table in tables {
        let query = DropTableStatement::new(dialect, &table.name).cascade().to_sql().0;

        adapter.execute(&query, &[]).await?
    }

    Ok(())
}

pub async fn create_migration_table<T: DinocoAdapter>(adapter: &T) -> DinocoResult<()> {
    let dialect = adapter.dialect();

    let stmt = CreateTableStatement::new(dialect, "_dinoco_migrations")
        .column(ColumnDefinition {
            name: "id",
            col_type: ColumnType::Integer,
            primary_key: true,
            not_null: true,
            auto_increment: true,
            default: None,
        })
        .column(ColumnDefinition {
            name: "name",
            col_type: ColumnType::Text,
            primary_key: false,
            not_null: true,
            auto_increment: false,
            default: None,
        })
        .column(ColumnDefinition {
            name: "schema",
            col_type: ColumnType::Bytes,
            primary_key: false,
            not_null: true,
            auto_increment: false,
            default: None,
        })
        .column(ColumnDefinition {
            name: "applied_at",
            col_type: ColumnType::DateTime,
            primary_key: false,
            not_null: true,
            auto_increment: false,
            default: Some(ColumnDefault::Function("NOW()".to_string())),
        });

    let (sql, _) = stmt.to_sql();

    adapter.execute(&sql, &[]).await?;

    Ok(())
}

pub async fn get_last_migration<T: DinocoAdapter>(adapter: &T) -> DinocoResult<Option<DinocoMigration>> {
    let result = adapter
        .query_as::<DinocoMigration>("SELECT id, name, schema FROM _dinoco_migrations ORDER BY id DESC LIMIT 1", &[])
        .await?;

    Ok(result.into_iter().next())
}

pub async fn insert_migration<T: DinocoAdapter>(adapter: &T, name: &str, schema_bytes: Vec<u8>) -> DinocoResult<()> {
    adapter
        .execute(
            "INSERT INTO _dinoco_migrations (name, schema) VALUES ($1, $2)",
            &[DinocoValue::String(name.to_string()), DinocoValue::Bytes(schema_bytes)],
        )
        .await?;

    Ok(())
}

pub async fn fetch<T: DinocoAdapter>(adapter: &T, db_type: &Database) -> DinocoResult<Vec<DatabaseParsedTable>> {
    let mut tables = vec![];

    let all_tables = fetch_tables(adapter, db_type).await?;

    for table in all_tables {
        let columns = fetch_columns(adapter, &table.name, db_type).await?;
        let foreign_keys = fetch_foreign_keys(adapter, &table.name, db_type).await?;

        tables.push(DatabaseParsedTable {
            name: table.name.clone(),
            columns,
            foreign_keys,
            primary_keys: vec![],
        })
    }

    Ok(tables)
}

pub async fn fetch_tables<T: DinocoAdapter>(adapter: &T, db_type: &Database) -> DinocoResult<Vec<DatabaseTable>> {
    let schema_filter = match db_type {
        Database::Postgresql => "table_schema = 'public'",
        Database::Mysql => "table_schema = DATABASE()",
    };

    let query = format!(
        r#"
        SELECT table_name AS name
        FROM information_schema.tables 
        WHERE {} AND table_type = 'BASE TABLE'
        "#,
        schema_filter
    );

    adapter.query_as::<DatabaseTable>(&query, &[]).await
}

pub async fn fetch_columns<T: DinocoAdapter>(adapter: &T, table_name: &str, db_type: &Database) -> DinocoResult<Vec<DatabaseColumn>> {
    let (schema_filter, bool_cast) = match db_type {
        Database::Postgresql => ("table_schema = 'public'", "CAST(is_nullable = 'YES' AS BOOLEAN)"),
        Database::Mysql => ("table_schema = DATABASE()", "is_nullable = 'YES'"), // MySQL converte 1/0 para bool nativamente
    };

    let query = format!(
        r#"
        SELECT 
            column_name AS name,
            data_type AS db_type,
            {} AS nullable,
            column_default AS "default"
        FROM information_schema.columns
        WHERE {} AND table_name = $1
        ORDER BY ordinal_position
        "#,
        bool_cast, schema_filter
    );

    adapter.query_as::<DatabaseColumn>(&query, &[DinocoValue::String(table_name.to_string())]).await
}

pub async fn fetch_foreign_keys<T: DinocoAdapter>(adapter: &T, table_name: &str, db_type: &Database) -> DinocoResult<Vec<DatabaseForeignKey>> {
    let schema_filter = match db_type {
        Database::Postgresql => "tc.table_schema = 'public'",
        Database::Mysql => "tc.table_schema = DATABASE()",
    };

    let query = format!(
        r#"
        SELECT
            kcu.column_name AS "column",
            ccu.table_name AS references_table,
            ccu.column_name AS references_column
        FROM information_schema.table_constraints tc
        JOIN information_schema.key_column_usage kcu
            ON tc.constraint_name = kcu.constraint_name
        JOIN information_schema.constraint_column_usage ccu
            ON ccu.constraint_name = tc.constraint_name
        WHERE tc.constraint_type = 'FOREIGN KEY'
          AND {}
          AND tc.table_name = $1
        "#,
        schema_filter
    );

    adapter.query_as::<DatabaseForeignKey>(&query, &[DinocoValue::String(table_name.to_string())]).await
}
