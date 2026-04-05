use dinoco_engine::{
    AdapterDialect, DeleteStatement, DinocoAdapter, DinocoResult, DinocoValue, Expression,
    InsertStatement, OrderDirection, QueryBuilder, SelectStatement,
};

use crate::DinocoMigration;

const MIGRATIONS_TABLE: &str = "_dinoco_migrations";

pub async fn get_all_migrations<T: DinocoAdapter>(
    adapter: &T,
) -> DinocoResult<Vec<DinocoMigration>> {
    let dialect = adapter.dialect();
    let stmt = SelectStatement::new()
        .select(&["name", "schema"])
        .from(MIGRATIONS_TABLE)
        .order_by("name", OrderDirection::Asc);

    let (query, params) = dialect.build_select(&stmt);

    adapter.query_as::<DinocoMigration>(&query, &params).await
}

pub async fn get_last_migration<T: DinocoAdapter>(
    adapter: &T,
) -> DinocoResult<Option<DinocoMigration>> {
    Ok(get_last_migrations(adapter, 1).await?.into_iter().next())
}

pub async fn get_last_two_migrations<T: DinocoAdapter>(
    adapter: &T,
) -> DinocoResult<Vec<DinocoMigration>> {
    get_last_migrations(adapter, 2).await
}

async fn get_last_migrations<T: DinocoAdapter>(
    adapter: &T,
    limit: usize,
) -> DinocoResult<Vec<DinocoMigration>> {
    let dialect = adapter.dialect();
    let stmt = SelectStatement::new()
        .select(&["name", "schema"])
        .from(MIGRATIONS_TABLE)
        .order_by("name", OrderDirection::Desc)
        .limit(limit);

    let (query, params) = dialect.build_select(&stmt);

    adapter.query_as::<DinocoMigration>(&query, &params).await
}

pub async fn create_migration_table<T: DinocoAdapter>(adapter: &T) -> DinocoResult<()> {
    let dialect = adapter.dialect();
    let name_type = "VARCHAR(255)";
    let schema_type = if dialect.bind_param(1) == "$1" {
        "BYTEA"
    } else {
        "BLOB"
    };

    let sql = format!(
        "CREATE TABLE IF NOT EXISTS {} ({} {} PRIMARY KEY NOT NULL, {} {} NOT NULL)",
        dialect.identifier(MIGRATIONS_TABLE),
        dialect.identifier("name"),
        name_type,
        dialect.identifier("schema"),
        schema_type,
    );

    adapter.execute(&sql, &[]).await
}

pub async fn insert_migration<T: DinocoAdapter>(
    adapter: &T,
    name: &str,
    schema_bytes: Vec<u8>,
) -> DinocoResult<()> {
    let dialect = adapter.dialect();
    let stmt = InsertStatement::new()
        .into(MIGRATIONS_TABLE)
        .columns(&["name", "schema"])
        .value(vec![
            DinocoValue::String(name.to_string()),
            DinocoValue::Bytes(schema_bytes),
        ]);

    let (query, params) = dialect.build_insert(&stmt);

    adapter.execute(&query, &params).await
}

pub async fn delete_migration<T: DinocoAdapter>(adapter: &T, name: &str) -> DinocoResult<()> {
    let dialect = adapter.dialect();
    let stmt = DeleteStatement::new()
        .from(MIGRATIONS_TABLE)
        .condition(Expression::Column("name".to_string()).eq(name.to_string()));

    let (query, params) = dialect.build_delete(&stmt);

    adapter.execute(&query, &params).await
}
