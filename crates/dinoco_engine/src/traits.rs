use async_trait::async_trait;

use crate::{DinocoValue, FindQuery, RelationBatchQuery, RelationJoinQuery, SqliteRow};

#[async_trait(?Send)]
pub trait DinocoEntity: Sized + Send + 'static {
    const TABLE_NAME: &'static str = "";
    const FIELDS: &'static [&'static str] = &[];

    type Where: Default;
    type OrderBy: Default;
    type Include: Default;

    // type Update;
}

#[async_trait(?Send)]
pub trait DinocoAdapter: Sized {
    async fn new(path: String) -> Result<Self, String>;

    async fn query<M>(&self, query: &str, params: &[DinocoValue]) -> anyhow::Result<Vec<M>>
    where
        M: DinocoSqlite;

    async fn query_optional<M>(&self, query: &str, params: &[DinocoValue]) -> anyhow::Result<Vec<M>>
    where
        M: DinocoSqlite;
}

pub trait DinocoSqlCompiler {
    fn compile_find_query(&self, query: FindQuery) -> (String, Vec<DinocoValue>);
    fn compile_relation_batch_query(&self, query: RelationBatchQuery) -> (String, Vec<DinocoValue>);
    fn compile_relation_join_query(&self, query: RelationJoinQuery) -> String;
}

pub trait DinocoSqlite: Sized + Send + 'static {
    fn from_sqlite_row(row: &SqliteRow<'_>) -> Option<Self>;
}

pub trait DinocoProjection<M>: DinocoSqlite {
    const FIELDS: &'static [&'static str];
}
