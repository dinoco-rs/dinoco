use chrono::Utc;

use dinoco_engine::{
    AdapterDialect, DinocoAdapter, DinocoResult, DinocoValue, Expression, InsertStatement, OrderDirection,
    QueryBuilder, SelectStatement, UpdateStatement,
};

use crate::DinocoMigration;

const MIGRATIONS_TABLE: &str = "_dinoco_migrations";

pub async fn get_all_migrations<T: DinocoAdapter>(adapter: &T) -> DinocoResult<Vec<DinocoMigration>> {
    let dialect = adapter.dialect();
    let stmt = SelectStatement::new()
        .select(&["name", "applied_at", "rollback_at"])
        .from(MIGRATIONS_TABLE)
        .order_by("name", OrderDirection::Asc);

    let (query, params) = dialect.build_select(&stmt);

    adapter.query_as::<DinocoMigration>(&query, &params).await
}

pub async fn get_last_migration<T: DinocoAdapter>(adapter: &T) -> DinocoResult<Option<DinocoMigration>> {
    Ok(get_last_migrations(adapter, 1).await?.into_iter().next())
}

async fn get_last_migrations<T: DinocoAdapter>(adapter: &T, limit: usize) -> DinocoResult<Vec<DinocoMigration>> {
    let dialect = adapter.dialect();
    let stmt = SelectStatement::new()
        .select(&["name", "applied_at", "rollback_at"])
        .from(MIGRATIONS_TABLE)
        .condition(Expression::Column("applied_at".to_string()).is_not_null())
        .condition(Expression::Column("rollback_at".to_string()).is_null())
        .order_by("name", OrderDirection::Desc)
        .limit(limit);

    let (query, params) = dialect.build_select(&stmt);

    adapter.query_as::<DinocoMigration>(&query, &params).await
}

pub async fn create_migration_table<T: DinocoAdapter>(adapter: &T) -> DinocoResult<()> {
    let dialect = adapter.dialect();
    let name_type = "VARCHAR(255)";
    let timestamp_type = "TIMESTAMP";

    let sql = format!(
        "CREATE TABLE IF NOT EXISTS {} ({} {} PRIMARY KEY NOT NULL, {} {} NULL, {} {} NULL)",
        dialect.identifier(MIGRATIONS_TABLE),
        dialect.identifier("name"),
        name_type,
        dialect.identifier("applied_at"),
        timestamp_type,
        dialect.identifier("rollback_at"),
        timestamp_type,
    );

    adapter.execute(&sql, &[]).await
}

pub async fn insert_migration<T: DinocoAdapter>(adapter: &T, name: &str) -> DinocoResult<()> {
    if get_migration_by_name(adapter, name).await?.is_some() {
        return Ok(());
    }

    let dialect = adapter.dialect();
    let stmt = InsertStatement::new()
        .into(MIGRATIONS_TABLE)
        .columns(&["name", "applied_at", "rollback_at"])
        .value(vec![DinocoValue::String(name.to_string()), DinocoValue::Null, DinocoValue::Null]);

    let (query, params) = dialect.build_insert(&stmt);

    adapter.execute(&query, &params).await
}

pub async fn get_migration_by_name<T: DinocoAdapter>(adapter: &T, name: &str) -> DinocoResult<Option<DinocoMigration>> {
    let dialect = adapter.dialect();
    let stmt = SelectStatement::new()
        .select(&["name", "applied_at", "rollback_at"])
        .from(MIGRATIONS_TABLE)
        .condition(Expression::Column("name".to_string()).eq(name.to_string()))
        .limit(1);

    let (query, params) = dialect.build_select(&stmt);
    let mut rows = adapter.query_as::<DinocoMigration>(&query, &params).await?;

    Ok(rows.pop())
}

pub async fn mark_migration_applied<T: DinocoAdapter>(adapter: &T, name: &str) -> DinocoResult<()> {
    insert_migration(adapter, name).await?;

    let dialect = adapter.dialect();
    let stmt = UpdateStatement::new()
        .table(MIGRATIONS_TABLE)
        .set("applied_at", DinocoValue::DateTime(Utc::now()))
        .set("rollback_at", DinocoValue::Null)
        .condition(Expression::Column("name".to_string()).eq(name.to_string()));

    let (query, params) = dialect.build_update(&stmt);

    adapter.execute(&query, &params).await
}
