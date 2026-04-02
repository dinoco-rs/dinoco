use dinoco_engine::{
    BinaryOperator, ColumnDefault, ColumnDefinition, ColumnType, CreateTableStatement, DeleteStatement, DinocoAdapter, DinocoResult, DinocoValue, DropTableStatement, Expression,
    InsertStatement, OrderDirection, SelectStatement, SqlDialect, SqlDialectBuilders,
};

use crate::{DatabaseColumn, DatabaseParsedTable, DatabaseTable, DinocoMigration};

pub async fn drop_all_tables<T: DinocoAdapter>(adapter: &T, tables: Vec<DatabaseParsedTable>) -> DinocoResult<()>
where
    T::Dialect: SqlDialectBuilders,
{
    let dialect = adapter.dialect();

    for table in tables {
        let stmt = DropTableStatement::new(dialect, &table.name).cascade();
        let (query, _) = dialect.build_drop_table(&stmt);

        adapter.execute(&query, &[]).await?;
    }

    Ok(())
}

pub async fn get_last_migration<T: DinocoAdapter>(adapter: &T) -> DinocoResult<Option<DinocoMigration>> {
    let (query, params) = SelectStatement::new(adapter.dialect())
        .select(&["id", "name", "schema"])
        .from("_dinoco_migrations")
        .order_by("id", OrderDirection::Desc)
        .limit(1)
        .to_sql();

    let result = adapter.query_as::<DinocoMigration>(&query, &params).await?;

    Ok(result.into_iter().next())
}

pub async fn get_last_two_migrations<T: DinocoAdapter>(adapter: &T) -> DinocoResult<Vec<DinocoMigration>> {
    let (query, params) = SelectStatement::new(adapter.dialect())
        .select(&["id", "name", "schema"])
        .from("_dinoco_migrations")
        .order_by("id", OrderDirection::Desc)
        .limit(2)
        .to_sql();

    match adapter.query_as::<DinocoMigration>(&query, &params).await {
        Ok(migrations) => Ok(migrations),
        Err(..) => Ok(vec![]),
    }
}

pub async fn create_migration_table<T: DinocoAdapter>(adapter: &T) -> DinocoResult<()>
where
    T::Dialect: SqlDialectBuilders,
{
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
    let (query, params) = InsertStatement::new(adapter.dialect())
        .into("_dinoco_migrations")
        .columns(&["name", "schema"])
        .value(vec![DinocoValue::String(name.to_string()), DinocoValue::Bytes(schema_bytes)])
        .to_sql();

    adapter.execute(&query, &params).await
}

pub async fn delete_migration<T: DinocoAdapter>(adapter: &T, id: i64) -> DinocoResult<()> {
    let (query, params) = DeleteStatement::new(adapter.dialect())
        .from("_dinoco_migrations")
        .condition(Expression::BinaryOp {
            left: Box::new(Expression::Column("id".to_string())),
            op: BinaryOperator::Eq,
            right: Box::new(Expression::Value(DinocoValue::Integer(id))),
        })
        .to_sql();

    adapter.execute(&query, &params).await
}

pub async fn fetch<T: DinocoAdapter>(adapter: &T) -> DinocoResult<Vec<DatabaseParsedTable>> {
    let mut tables = vec![];
    let all_tables = fetch_tables(adapter).await?;

    for table in all_tables {
        let columns = fetch_columns(adapter, &table.name).await?;

        tables.push(DatabaseParsedTable {
            name: table.name.clone(),
            columns,
        })
    }

    Ok(tables)
}

pub async fn fetch_tables<T: DinocoAdapter>(adapter: &T) -> DinocoResult<Vec<DatabaseTable>> {
    let dialect = adapter.dialect();

    let (query, params) = SelectStatement::new(dialect)
        .select(&["table_name as name"])
        .from("information_schema.tables")
        .condition(Expression::BinaryOp {
            left: Box::new(Expression::Column("table_schema".to_string())),
            op: BinaryOperator::Eq,
            right: Box::new(Expression::String(dialect.default_schema())),
        })
        .condition(Expression::BinaryOp {
            left: Box::new(Expression::Column("table_type".to_string())),
            op: BinaryOperator::Eq,
            right: Box::new(Expression::String("BASE TABLE".to_string())),
        })
        .to_sql();

    adapter.query_as::<DatabaseTable>(&query, &params).await
}

pub async fn fetch_columns<T: DinocoAdapter>(adapter: &T, table_name: &str) -> DinocoResult<Vec<DatabaseColumn>> {
    let dialect = adapter.dialect();

    let nullable = format!("{} AS nullable", dialect.cast_boolean("is_nullable"));
    let fields = &["column_name AS name", "data_type AS db_type", nullable.as_str(), "column_default AS default_value"];

    let (query, params) = SelectStatement::new(dialect)
        .select(fields)
        .from("information_schema.columns")
        .condition(Expression::BinaryOp {
            left: Box::new(Expression::Column("table_schema".to_string())),
            op: BinaryOperator::Eq,
            right: Box::new(Expression::String(dialect.default_schema())),
        })
        .condition(Expression::BinaryOp {
            left: Box::new(Expression::Column("table_name".to_string())),
            op: BinaryOperator::Eq,
            right: Box::new(Expression::Value(DinocoValue::String(table_name.to_string()))),
        })
        .to_sql();

    adapter.query_as::<DatabaseColumn>(&query, &params).await
}
