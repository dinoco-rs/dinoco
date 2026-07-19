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
                let Some(returning) = query.returning else {
                    let (sql, params) = adapter.compile_insert_query(query);

                    println!("SQL: {}, params: {:#?}", sql, params);

                    return adapter.query::<M>(&sql, &params).await;
                };

                let id_index = query
                    .fields
                    .iter()
                    .position(|field| *field == "id")
                    .ok_or_else(|| anyhow::anyhow!("MySQL insert returning fallback requires an `id` field."))?;
                let ids = query.rows.iter().map(|row| row[id_index].clone()).collect::<Vec<_>>();
                let mut insert_query = query;
                insert_query.returning = None;
                let table = insert_query.table;
                let (sql, params) = adapter.compile_insert_query(insert_query);

                println!("SQL: {}, params: {:#?}", sql, params);

                adapter.execute(&sql, &params).await?;

                let find = FindQuery {
                    fields: returning,
                    from: table,
                    conditions: vec![crate::FindWhere::Batch("id", ids)],
                    limit: -1,
                    skip: -1,
                    order_by: None,
                };
                let (sql, params) = adapter.compile_find_query(find);

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
                let Some(returning) = query.returning else {
                    let (sql, params) = adapter.compile_update_query(query);

                    println!("SQL: {}, params: {:#?}", sql, params);

                    return adapter.query::<M>(&sql, &params).await;
                };

                let conditions = query.conditions.clone();
                let id_lookup = FindQuery {
                    fields: &["id"],
                    from: query.table,
                    conditions: conditions.clone(),
                    limit: -1,
                    skip: -1,
                    order_by: None,
                };
                let (sql, params) = adapter.compile_find_query(id_lookup);

                println!("SQL: {}, params: {:#?}", sql, params);

                let ids = adapter
                    .query::<crate::SingleIdRow>(&sql, &params)
                    .await?
                    .into_iter()
                    .map(|row| row.id)
                    .collect::<Vec<_>>();

                let mut update_query = query;
                update_query.returning = None;
                let table = update_query.table;
                let (sql, params) = adapter.compile_update_query(update_query);

                println!("SQL: {}, params: {:#?}", sql, params);

                adapter.execute(&sql, &params).await?;

                if ids.is_empty() {
                    return Ok(Vec::new());
                }

                let find = FindQuery {
                    fields: returning,
                    from: table,
                    conditions: vec![crate::FindWhere::Batch("id", ids)],
                    limit: -1,
                    skip: -1,
                    order_by: None,
                };
                let (sql, params) = adapter.compile_find_query(find);

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
                let Some(returning) = query.returning else {
                    let (sql, params) = adapter.compile_delete_query(query);

                    println!("SQL: {}, params: {:#?}", sql, params);

                    return adapter.query::<M>(&sql, &params).await;
                };

                let find = FindQuery {
                    fields: returning,
                    from: query.table,
                    conditions: query.conditions.clone(),
                    limit: -1,
                    skip: -1,
                    order_by: None,
                };
                let (sql, params) = adapter.compile_find_query(find);

                println!("SQL: {}, params: {:#?}", sql, params);

                let rows = adapter.query::<M>(&sql, &params).await?;
                let mut delete_query = query;
                delete_query.returning = None;
                let (sql, params) = adapter.compile_delete_query(delete_query);

                println!("SQL: {}, params: {:#?}", sql, params);

                adapter.execute(&sql, &params).await?;

                Ok(rows)
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
