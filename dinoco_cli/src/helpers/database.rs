use dinoco_engine::{ColumnDefault, ColumnDefinition, ColumnType, DinocoAdapter, DinocoResult, DinocoValue};

use crate::DinocoMigration;

pub async fn get_all_migrations<T: DinocoAdapter>(adapter: &T) -> DinocoResult<Option<DinocoMigration>> {
    let dialect = adapter.dialect();

    let stmt = SelectStatement::new(dialect)
        .select(&["id", "name", "schema"])
        .from("_dinoco_migrations")
        .order_by("id", OrderDirection::Desc);

    let (query, params) = dialect.build_select(&stmt);
    let result = adapter.query_as::<DinocoMigration>(&query, &params).await?;

    Ok(result.into_iter().next())
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

    let (sql, _) = dialect.build_create_table(&stmt);

    adapter.execute(&sql, &[]).await?;

    Ok(())
}

pub async fn insert_migration<T: DinocoAdapter>(adapter: &T, name: &str, schema_bytes: Vec<u8>) -> DinocoResult<()> {
    let dialect = adapter.dialect();

    let stmt = InsertStatement::new(dialect)
        .into("_dinoco_migrations")
        .columns(&["name", "schema"])
        .value(vec![DinocoValue::String(name.to_string()), DinocoValue::Bytes(schema_bytes)]);

    let (query, params) = dialect.build_insert(&stmt);

    adapter.execute(&query, &params).await
}

pub async fn delete_migration<T: DinocoAdapter>(adapter: &T, id: i64) -> DinocoResult<()> {
    let dialect = adapter.dialect();

    let stmt = DeleteStatement::new(dialect).from("_dinoco_migrations").condition(Expression::BinaryOp {
        left: Box::new(Expression::Column("id".to_string())),
        op: BinaryOperator::Eq,
        right: Box::new(Expression::Value(DinocoValue::Integer(id))),
    });

    let (query, params) = dialect.build_delete(&stmt);

    adapter.execute(&query, &params).await
}
