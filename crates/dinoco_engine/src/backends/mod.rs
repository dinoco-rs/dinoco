mod sqlite;

pub use sqlite::*;

use crate::{DinocoAdapter, DinocoSqlCompiler, DinocoSqlite, FindQuery, RelationBatchQuery, RelationJoinQuery};

pub enum Backend {
    Sqlite(SqliteAdapter),
}

impl Backend {
    pub async fn query<M>(&self, query: FindQuery) -> anyhow::Result<Vec<M>>
    where
        M: DinocoSqlite,
    {
        match &self {
            Backend::Sqlite(adapter) => {
                let (sql, params) = adapter.compile_find_query(query);

                println!("SQL: {}, params: {:#?}", sql, params);

                adapter.query::<M>(&sql, &params).await
            }
        }
    }

    pub async fn query_relation_join_optional<M>(
        &self,
        query: RelationJoinQuery,
        params: &[crate::DinocoValue],
    ) -> anyhow::Result<Vec<M>>
    where
        M: DinocoSqlite,
    {
        match &self {
            Backend::Sqlite(adapter) => {
                let sql = adapter.compile_relation_join_query(query);

                println!("SQL: {}, params: {:#?}", sql, params);

                adapter.query_optional::<M>(&sql, params).await
            }
        }
    }

    pub async fn query_relation_batch<M>(&self, query: RelationBatchQuery) -> anyhow::Result<Vec<M>>
    where
        M: DinocoSqlite,
    {
        match &self {
            Backend::Sqlite(adapter) => {
                let (sql, params) = adapter.compile_relation_batch_query(query);

                println!("SQL: {}, params: {:#?}", sql, params);

                adapter.query::<M>(&sql, &params).await
            }
        }
    }
}
