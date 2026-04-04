use dinoco_compiler::ParsedTable;
use dinoco_derives::Seriable;
use dinoco_engine::{
    AlterTableStatement, BinaryOperator, ColumnDefault, ColumnDefinition, ColumnType, CreateTableStatement, DeleteStatement, DinocoAdapter, DinocoResult, DinocoValue,
    DropEnumStatement, DropTableStatement, Expression, InsertStatement, OrderDirection, SelectStatement, SqlDialect, SqlDialectBuilders,
};

use crate::{
    DatabaseColumn, DatabaseParsedTable, DatabaseTable, DinocoMigration,
    helpers::{DatabaseEnum, DatabaseForeignKey},
};

pub async fn drop_all_tables<T: DinocoAdapter>(adapter: &T, tables: Vec<DatabaseParsedTable>) -> DinocoResult<()> {
    let dialect = adapter.dialect();

    if dialect.supports_drop_constraints() {
        let foreign_keys = adapter.query_as::<DatabaseForeignKey>(&dialect.query_get_foreign_keys(), &[]).await?;

        for key in foreign_keys {
            let parsed_table = ParsedTable {
                name: key.table_name.clone(),
                fields: vec![],
            };

            let stmt = AlterTableStatement::new(dialect, &key.table_name).drop_constraint(parsed_table, vec![], &key.constraint_name);

            for (query, params) in dialect.build_alter_table(&stmt) {
                adapter.execute(&query, &params).await?;
            }
        }
    }

    for table in tables {
        let stmt = DropTableStatement::new(dialect, &table.name).cascade();
        let (query, params) = dialect.build_drop_table(&stmt);

        adapter.execute(&query, &params).await?;
    }

    if dialect.supports_native_enums() {
        let enums = adapter.query_as::<DatabaseEnum>(&dialect.query_get_enums(), &[]).await?;

        for en in enums {
            let stmt = DropEnumStatement::new(dialect, &en.name).cascade();
            let (query, params) = dialect.build_drop_enum(&stmt);

            adapter.execute(&query, &params).await?;
        }
    }

    Ok(())
}

pub async fn get_last_migration<T: DinocoAdapter>(adapter: &T) -> DinocoResult<Option<DinocoMigration>> {
    let dialect = adapter.dialect();

    let stmt = SelectStatement::new(dialect)
        .select(&["id", "name", "schema"])
        .from("_dinoco_migrations")
        .order_by("id", OrderDirection::Desc)
        .limit(1);

    let (query, params) = dialect.build_select(&stmt);
    let result = adapter.query_as::<DinocoMigration>(&query, &params).await?;

    Ok(result.into_iter().next())
}

pub async fn get_last_two_migrations<T: DinocoAdapter>(adapter: &T) -> DinocoResult<Vec<DinocoMigration>> {
    let dialect = adapter.dialect();
    let stmt = SelectStatement::new(dialect)
        .select(&["id", "name", "schema"])
        .from("_dinoco_migrations")
        .order_by("id", OrderDirection::Desc)
        .limit(2);

    let (query, params) = dialect.build_select(&stmt);

    match adapter.query_as::<DinocoMigration>(&query, &params).await {
        Ok(migrations) => Ok(migrations),
        Err(..) => Ok(vec![]),
    }
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

    println!("{:?}", sql);

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

    // let stmt = SelectStatement::new(dialect)
    //     .select(&["table_name as name"])
    //     .from("information_schema.tables")
    //     .condition(Expression::BinaryOp {
    //         left: Box::new(Expression::Column("table_schema".to_string())),
    //         op: BinaryOperator::Eq,
    //         right: Box::new(Expression::String(dialect.default_schema())),
    //     })
    //     .condition(Expression::BinaryOp {
    //         left: Box::new(Expression::Column("table_type".to_string())),
    //         op: BinaryOperator::Eq,
    //         right: Box::new(Expression::String("BASE TABLE".to_string())),
    //     });

    // let (query, params) = dialect.build_select(&stmt);

    // adapter.query_as::<DatabaseTable>(&query, &params).await

    let query = r#"
        SELECT name
        FROM sqlite_master
        WHERE type = 'table'
        AND name NOT LIKE 'sqlite_%'
    "#;

    let result = adapter.query_as::<DatabaseTable>(query, &[]).await;

    println!("{:?}", result);

    return result;
}

pub async fn fetch_columns<T: DinocoAdapter>(adapter: &T, table_name: &str) -> DinocoResult<Vec<DatabaseColumn>> {
    let dialect = adapter.dialect();

    // let nullable = format!("{} AS nullable", dialect.cast_boolean("is_nullable"));
    // let fields = &["column_name AS name", "data_type AS db_type", nullable.as_str(), "column_default AS default_value"];

    // let stmt = SelectStatement::new(dialect)
    //     .select(fields)
    //     .from("information_schema.columns")
    //     .condition(Expression::BinaryOp {
    //         left: Box::new(Expression::Column("table_schema".to_string())),
    //         op: BinaryOperator::Eq,
    //         right: Box::new(Expression::String(dialect.default_schema())),
    //     })
    //     .condition(Expression::BinaryOp {
    //         left: Box::new(Expression::Column("table_name".to_string())),
    //         op: BinaryOperator::Eq,
    //         right: Box::new(Expression::Value(DinocoValue::String(table_name.to_string()))),
    //     });

    // let (query, params) = dialect.build_select(&stmt);

    // adapter.query_as::<DatabaseColumn>(&query, &params).await

    let query = format!("PRAGMA table_info({})", dialect.identifier(table_name));

    // #[derive(Seriable)]
    // pub struct SqliteColumnRaw {
    //     pub name: String,
    //     pub r#type: String,
    //     pub notnull: i64,
    //     pub dflt_value: Option<String>,
    // }

    // let rows = adapter.query_as::<SqliteColumnRaw>(&query, &[]).await?;

    // let result = rows
    //     .into_iter()
    //     .map(|col| DatabaseColumn {
    //         name: col.name,
    //         db_type: col.r#type,
    //         nullable: col.notnull == 0,
    //         default_value: col.dflt_value,
    //     })
    //     .collect();

    // println!("{}")

    // unimplemented!()

    Ok(vec![])
}
