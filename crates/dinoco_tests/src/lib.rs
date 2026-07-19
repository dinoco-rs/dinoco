use dinoco_engine::{
    CreateTableMigration, DinocoAdapter, DinocoSqlCompiler, DropTableMigration, MigrationColumn, MigrationColumnType,
    MigrationDefault,
};

pub fn column(name: &str, ty: MigrationColumnType) -> MigrationColumn {
    MigrationColumn { name: name.to_string(), ty, primary_key: false, nullable: false, default: None }
}

pub fn primary(mut column: MigrationColumn) -> MigrationColumn {
    column.primary_key = true;
    column
}

pub fn nullable(mut column: MigrationColumn) -> MigrationColumn {
    column.nullable = true;
    column
}

pub fn default(mut column: MigrationColumn, value: MigrationDefault) -> MigrationColumn {
    column.default = Some(value);
    column
}

pub async fn create_table<A>(adapter: &A, table: &str, columns: Vec<MigrationColumn>) -> anyhow::Result<()>
where
    A: DinocoAdapter + DinocoSqlCompiler,
{
    let sql = adapter.compile_create_table_migration(CreateTableMigration {
        table: table.to_string(),
        columns,
        foreign_keys: Vec::new(),
        if_not_exists: false,
    });
    adapter.execute(&sql, &[]).await?;
    Ok(())
}

pub async fn drop_table<A>(adapter: &A, table: &str) -> anyhow::Result<()>
where
    A: DinocoAdapter + DinocoSqlCompiler,
{
    let sql = adapter.compile_drop_table_migration(DropTableMigration { table: table.to_string(), if_exists: true });
    adapter.execute(&sql, &[]).await?;
    Ok(())
}
