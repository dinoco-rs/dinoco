mod mysql;
mod postgres;
mod sqlite;

pub use mysql::*;
pub use postgres::*;
pub use sqlite::*;

use crate::{
    CountQuery, DeleteQuery, DinocoAdapter, DinocoRowModel, DinocoSqlCompiler, FindQuery, InsertQuery,
    RelationBatchQuery, RelationCountQuery, RelationJoinQuery, UpdateQuery,
};

pub enum Backend {
    Sqlite(SqliteAdapter),
    Postgres(PostgresAdapter),
    PgBouncer(PgBouncerAdapter),
    Mysql(MySqlAdapter),
}

impl Backend {
    pub async fn query<M>(&self, query: FindQuery) -> anyhow::Result<Vec<M>>
    where
        M: DinocoRowModel,
    {
        match &self {
            Backend::Sqlite(adapter) => {
                let (sql, params) = adapter.compile_find_query(query);

                println!("SQL: {}, params: {:#?}", sql, params);

                adapter.query::<M>(&sql, &params).await
            }
            Backend::Postgres(adapter) => {
                let (sql, params) = adapter.compile_find_query(query);

                println!("SQL: {}, params: {:#?}", sql, params);

                adapter.query::<M>(&sql, &params).await
            }
            Backend::PgBouncer(adapter) => {
                let (sql, params) = adapter.compile_find_query(query);

                println!("SQL: {}, params: {:#?}", sql, params);

                adapter.query::<M>(&sql, &params).await
            }
            Backend::Mysql(adapter) => {
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
        M: DinocoRowModel,
    {
        match &self {
            Backend::Sqlite(adapter) => {
                let (sql, extra_params) = adapter.compile_relation_join_query(query);
                let mut params = params.to_vec();
                params.extend(extra_params);

                println!("SQL: {}, params: {:#?}", sql, params);

                adapter.query_optional::<M>(&sql, &params).await
            }
            Backend::Postgres(adapter) => {
                let (sql, extra_params) = adapter.compile_relation_join_query(query);
                let mut params = params.to_vec();
                params.extend(extra_params);

                println!("SQL: {}, params: {:#?}", sql, params);

                adapter.query_optional::<M>(&sql, &params).await
            }
            Backend::PgBouncer(adapter) => {
                let (sql, extra_params) = adapter.compile_relation_join_query(query);
                let mut params = params.to_vec();
                params.extend(extra_params);

                println!("SQL: {}, params: {:#?}", sql, params);

                adapter.query_optional::<M>(&sql, &params).await
            }
            Backend::Mysql(adapter) => {
                let (sql, extra_params) = adapter.compile_relation_join_query(query);
                let mut params = params.to_vec();
                params.extend(extra_params);

                println!("SQL: {}, params: {:#?}", sql, params);

                adapter.query_optional::<M>(&sql, &params).await
            }
        }
    }

    pub async fn query_relation_batch<M>(&self, query: RelationBatchQuery) -> anyhow::Result<Vec<M>>
    where
        M: DinocoRowModel,
    {
        match &self {
            Backend::Sqlite(adapter) => {
                let (sql, params) = adapter.compile_relation_batch_query(query);

                println!("SQL: {}, params: {:#?}", sql, params);

                adapter.query::<M>(&sql, &params).await
            }
            Backend::Postgres(adapter) => {
                let (sql, params) = adapter.compile_relation_batch_query(query);

                println!("SQL: {}, params: {:#?}", sql, params);

                adapter.query::<M>(&sql, &params).await
            }
            Backend::PgBouncer(adapter) => {
                let (sql, params) = adapter.compile_relation_batch_query(query);

                println!("SQL: {}, params: {:#?}", sql, params);

                adapter.query::<M>(&sql, &params).await
            }
            Backend::Mysql(adapter) => {
                let (sql, params) = adapter.compile_relation_batch_query(query);

                println!("SQL: {}, params: {:#?}", sql, params);

                adapter.query::<M>(&sql, &params).await
            }
        }
    }

    pub async fn insert(&self, query: InsertQuery) -> anyhow::Result<usize> {
        match &self {
            Backend::Sqlite(adapter) => {
                let (sql, params) = adapter.compile_insert_query(query);

                println!("SQL: {}, params: {:#?}", sql, params);

                adapter.execute(&sql, &params).await
            }
            Backend::Postgres(adapter) => {
                let (sql, params) = adapter.compile_insert_query(query);

                println!("SQL: {}, params: {:#?}", sql, params);

                adapter.execute(&sql, &params).await
            }
            Backend::PgBouncer(adapter) => {
                let (sql, params) = adapter.compile_insert_query(query);

                println!("SQL: {}, params: {:#?}", sql, params);

                adapter.execute(&sql, &params).await
            }
            Backend::Mysql(adapter) => {
                let (sql, params) = adapter.compile_insert_query(query);

                println!("SQL: {}, params: {:#?}", sql, params);

                adapter.execute(&sql, &params).await
            }
        }
    }

    pub async fn insert_returning<M>(&self, query: InsertQuery) -> anyhow::Result<Vec<M>>
    where
        M: DinocoRowModel,
    {
        match &self {
            Backend::Sqlite(adapter) => {
                let (sql, params) = adapter.compile_insert_query(query);

                println!("SQL: {}, params: {:#?}", sql, params);

                adapter.query::<M>(&sql, &params).await
            }
            Backend::Postgres(adapter) => {
                let (sql, params) = adapter.compile_insert_query(query);

                println!("SQL: {}, params: {:#?}", sql, params);

                adapter.query::<M>(&sql, &params).await
            }
            Backend::PgBouncer(adapter) => {
                let (sql, params) = adapter.compile_insert_query(query);

                println!("SQL: {}, params: {:#?}", sql, params);

                adapter.query::<M>(&sql, &params).await
            }
            Backend::Mysql(adapter) => {
                let (sql, params) = adapter.compile_insert_query(query);

                println!("SQL: {}, params: {:#?}", sql, params);

                adapter.query::<M>(&sql, &params).await
            }
        }
    }

    pub async fn update(&self, query: UpdateQuery) -> anyhow::Result<usize> {
        match &self {
            Backend::Sqlite(adapter) => {
                let (sql, params) = adapter.compile_update_query(query);

                println!("SQL: {}, params: {:#?}", sql, params);

                adapter.execute(&sql, &params).await
            }
            Backend::Postgres(adapter) => {
                let (sql, params) = adapter.compile_update_query(query);

                println!("SQL: {}, params: {:#?}", sql, params);

                adapter.execute(&sql, &params).await
            }
            Backend::PgBouncer(adapter) => {
                let (sql, params) = adapter.compile_update_query(query);

                println!("SQL: {}, params: {:#?}", sql, params);

                adapter.execute(&sql, &params).await
            }
            Backend::Mysql(adapter) => {
                let (sql, params) = adapter.compile_update_query(query);

                println!("SQL: {}, params: {:#?}", sql, params);

                adapter.execute(&sql, &params).await
            }
        }
    }

    pub async fn update_returning<M>(&self, query: UpdateQuery) -> anyhow::Result<Vec<M>>
    where
        M: DinocoRowModel,
    {
        match &self {
            Backend::Sqlite(adapter) => {
                let (sql, params) = adapter.compile_update_query(query);

                println!("SQL: {}, params: {:#?}", sql, params);

                adapter.query::<M>(&sql, &params).await
            }
            Backend::Postgres(adapter) => {
                let (sql, params) = adapter.compile_update_query(query);

                println!("SQL: {}, params: {:#?}", sql, params);

                adapter.query::<M>(&sql, &params).await
            }
            Backend::PgBouncer(adapter) => {
                let (sql, params) = adapter.compile_update_query(query);

                println!("SQL: {}, params: {:#?}", sql, params);

                adapter.query::<M>(&sql, &params).await
            }
            Backend::Mysql(adapter) => {
                let (sql, params) = adapter.compile_update_query(query);

                println!("SQL: {}, params: {:#?}", sql, params);

                adapter.query::<M>(&sql, &params).await
            }
        }
    }

    pub async fn delete(&self, query: DeleteQuery) -> anyhow::Result<usize> {
        match &self {
            Backend::Sqlite(adapter) => {
                let (sql, params) = adapter.compile_delete_query(query);

                println!("SQL: {}, params: {:#?}", sql, params);

                adapter.execute(&sql, &params).await
            }
            Backend::Postgres(adapter) => {
                let (sql, params) = adapter.compile_delete_query(query);

                println!("SQL: {}, params: {:#?}", sql, params);

                adapter.execute(&sql, &params).await
            }
            Backend::PgBouncer(adapter) => {
                let (sql, params) = adapter.compile_delete_query(query);

                println!("SQL: {}, params: {:#?}", sql, params);

                adapter.execute(&sql, &params).await
            }
            Backend::Mysql(adapter) => {
                let (sql, params) = adapter.compile_delete_query(query);

                println!("SQL: {}, params: {:#?}", sql, params);

                adapter.execute(&sql, &params).await
            }
        }
    }

    pub async fn delete_returning<M>(&self, query: DeleteQuery) -> anyhow::Result<Vec<M>>
    where
        M: DinocoRowModel,
    {
        match &self {
            Backend::Sqlite(adapter) => {
                let (sql, params) = adapter.compile_delete_query(query);

                println!("SQL: {}, params: {:#?}", sql, params);

                adapter.query::<M>(&sql, &params).await
            }
            Backend::Postgres(adapter) => {
                let (sql, params) = adapter.compile_delete_query(query);

                println!("SQL: {}, params: {:#?}", sql, params);

                adapter.query::<M>(&sql, &params).await
            }
            Backend::PgBouncer(adapter) => {
                let (sql, params) = adapter.compile_delete_query(query);

                println!("SQL: {}, params: {:#?}", sql, params);

                adapter.query::<M>(&sql, &params).await
            }
            Backend::Mysql(adapter) => {
                let (sql, params) = adapter.compile_delete_query(query);

                println!("SQL: {}, params: {:#?}", sql, params);

                adapter.query::<M>(&sql, &params).await
            }
        }
    }

    pub async fn count(&self, query: CountQuery) -> anyhow::Result<i64> {
        match &self {
            Backend::Sqlite(adapter) => {
                let (sql, params) = adapter.compile_count_query(query);

                println!("SQL: {}, params: {:#?}", sql, params);

                adapter.query_count(&sql, &params).await
            }
            Backend::Postgres(adapter) => {
                let (sql, params) = adapter.compile_count_query(query);

                println!("SQL: {}, params: {:#?}", sql, params);

                adapter.query_count(&sql, &params).await
            }
            Backend::PgBouncer(adapter) => {
                let (sql, params) = adapter.compile_count_query(query);

                println!("SQL: {}, params: {:#?}", sql, params);

                adapter.query_count(&sql, &params).await
            }
            Backend::Mysql(adapter) => {
                let (sql, params) = adapter.compile_count_query(query);

                println!("SQL: {}, params: {:#?}", sql, params);

                adapter.query_count(&sql, &params).await
            }
        }
    }

    pub async fn count_relation(&self, query: RelationCountQuery) -> anyhow::Result<i64> {
        match &self {
            Backend::Sqlite(adapter) => {
                let (sql, params) = adapter.compile_relation_count_query(query);

                println!("SQL: {}, params: {:#?}", sql, params);

                adapter.query_count(&sql, &params).await
            }
            Backend::Postgres(adapter) => {
                let (sql, params) = adapter.compile_relation_count_query(query);

                println!("SQL: {}, params: {:#?}", sql, params);

                adapter.query_count(&sql, &params).await
            }
            Backend::PgBouncer(adapter) => {
                let (sql, params) = adapter.compile_relation_count_query(query);

                println!("SQL: {}, params: {:#?}", sql, params);

                adapter.query_count(&sql, &params).await
            }
            Backend::Mysql(adapter) => {
                let (sql, params) = adapter.compile_relation_count_query(query);

                println!("SQL: {}, params: {:#?}", sql, params);

                adapter.query_count(&sql, &params).await
            }
        }
    }
}
