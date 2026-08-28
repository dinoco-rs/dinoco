use async_trait::async_trait;

use crate::{
    AddColumnMigration, AddForeignKeyMigration, AlterColumnMigration, AlterEnumMigration, CountQuery,
    CreateEnumMigration, CreateIndexMigration, CreateTableMigration, DeadpoolPostgresRow, DeleteQuery, DinocoValue,
    DropColumnMigration, DropEnumMigration, DropForeignKeyMigration, DropIndexMigration, DropTableMigration, FindQuery,
    InsertQuery, ManyToManyRelationCountQuery, ManyToManyRelationQuery, MysqlRow, PostgresRow, RelationBatchQuery,
    RelationCountQuery, RelationJoinQuery, RenameColumnMigration, RenameTableMigration, SqliteRow, UpdateQuery,
};

#[async_trait]
pub trait DinocoEntity: Sized + Send + Sync + 'static {
    const TABLE_NAME: &'static str = "";
    const FIELDS: &'static [&'static str] = &[];

    type Where: Default;
    type OrderBy: Default;
    type Include: Default;
    type Update: Default;
    type Count: Default;
    type CountInclude: Default;
}

#[async_trait]
pub trait DinocoAdapter: Sized {
    async fn new(path: String) -> Result<Self, String>;

    async fn query<M>(&self, query: &str, params: &[DinocoValue]) -> anyhow::Result<Vec<M>>
    where
        M: DinocoRowModel;

    async fn query_optional<M>(&self, query: &str, params: &[DinocoValue]) -> anyhow::Result<Vec<M>>
    where
        M: DinocoRowModel;

    async fn execute(&self, query: &str, params: &[DinocoValue]) -> anyhow::Result<usize>;
}

pub trait DinocoSqlCompiler {
    fn compile_find_query(&self, query: FindQuery) -> (String, Vec<DinocoValue>);
    fn compile_insert_query(&self, query: InsertQuery) -> (String, Vec<DinocoValue>);
    fn compile_update_query(&self, query: UpdateQuery) -> (String, Vec<DinocoValue>);
    fn compile_delete_query(&self, query: DeleteQuery) -> (String, Vec<DinocoValue>);
    fn compile_count_query(&self, query: CountQuery) -> (String, Vec<DinocoValue>);
    fn compile_relation_count_query(&self, query: RelationCountQuery) -> (String, Vec<DinocoValue>);
    fn compile_relation_batch_query(&self, query: RelationBatchQuery) -> (String, Vec<DinocoValue>);
    fn compile_relation_join_query(&self, query: RelationJoinQuery) -> (String, Vec<DinocoValue>);
    fn compile_many_to_many_relation_query(&self, query: ManyToManyRelationQuery) -> (String, Vec<DinocoValue>);
    fn compile_many_to_many_relation_count_query(
        &self,
        query: ManyToManyRelationCountQuery,
    ) -> (String, Vec<DinocoValue>);
    fn compile_create_migrations_table(&self) -> String;
    fn compile_insert_migration_record(&self, name: &str) -> String;
    fn compile_create_table_migration(&self, migration: CreateTableMigration) -> String;
    fn compile_drop_table_migration(&self, migration: DropTableMigration) -> String;
    fn compile_rename_table_migration(&self, migration: RenameTableMigration) -> Vec<String>;
    fn compile_add_column_migration(&self, migration: AddColumnMigration) -> String;
    fn compile_drop_column_migration(&self, migration: DropColumnMigration) -> String;
    fn compile_alter_column_migration(&self, migration: AlterColumnMigration) -> Vec<String>;
    fn compile_rename_column_migration(&self, migration: RenameColumnMigration) -> Vec<String>;
    fn compile_add_foreign_key_migration(&self, migration: AddForeignKeyMigration) -> Vec<String>;
    fn compile_add_unvalidated_foreign_key_migration(&self, migration: AddForeignKeyMigration) -> Vec<String> {
        self.compile_add_foreign_key_migration(migration)
    }
    fn compile_drop_foreign_key_migration(&self, migration: DropForeignKeyMigration) -> Vec<String>;
    fn compile_create_index_migration(&self, migration: CreateIndexMigration) -> String;
    fn compile_drop_index_migration(&self, migration: DropIndexMigration) -> String;
    fn compile_create_enum_migration(&self, migration: CreateEnumMigration) -> Vec<String>;
    fn compile_drop_enum_migration(&self, migration: DropEnumMigration) -> Vec<String>;
    fn compile_alter_enum_migration(&self, migration: AlterEnumMigration) -> Vec<String>;
}

pub trait DinocoSqlite: Sized + Send + Sync + 'static {
    fn from_sqlite_row(row: &SqliteRow<'_>) -> Option<Self>;
}

pub trait DinocoPostgres: Sized + Send + Sync + 'static {
    fn from_deadpool_posgres_row(row: &DeadpoolPostgresRow) -> Option<Self>;
    fn from_deadpool_postgres_row(row: &DeadpoolPostgresRow) -> Option<Self> {
        Self::from_deadpool_posgres_row(row)
    }
    fn from_postgres_row(row: &PostgresRow) -> Option<Self>;
}

pub trait DinocoMysql: Sized + Send + Sync + 'static {
    fn from_mysql_row(row: &MysqlRow) -> Option<Self>;
}

pub trait DinocoRowModel: DinocoSqlite + DinocoPostgres + DinocoMysql {}

impl<T> DinocoRowModel for T where T: DinocoSqlite + DinocoPostgres + DinocoMysql {}

pub trait DinocoProjection<M>: DinocoRowModel {
    const FIELDS: &'static [&'static str];
}
