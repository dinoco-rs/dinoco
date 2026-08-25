mod mysql;
mod postgres;
mod sqlite;

pub use mysql::*;
pub use postgres::*;
pub use sqlite::*;

use crate::{
    CompiledTransactionCommand, CountQuery, DeleteQuery, DinocoAdapter, DinocoRowModel, DinocoSqlCompiler, FindQuery,
    InsertQuery, ManyToManyRelationCountQuery, ManyToManyRelationQuery, RelationBatchQuery, RelationCountQuery,
    RelationJoinQuery, TransactionCommand, TransactionResults, UpdateQuery,
};

pub enum Backend {
    Sqlite(SqliteAdapter),
    Postgres(PostgresAdapter),
    PgBouncer(PgBouncerAdapter),
    Mysql(MySqlAdapter),
}

impl Backend {
    pub async fn begin_transaction(&self) -> anyhow::Result<crate::TransactionExecutor> {
        let sender = match self {
            Backend::Sqlite(adapter) => adapter.begin_live_transaction().await?,
            Backend::Postgres(adapter) => adapter.begin_live_transaction().await?,
            Backend::PgBouncer(adapter) => adapter.inner().begin_live_transaction().await?,
            Backend::Mysql(adapter) => adapter.begin_live_transaction().await?,
        };
        Ok(crate::TransactionExecutor { sender, mysql: matches!(self, Backend::Mysql(_)) })
    }

    pub fn set_logger(&mut self, enabled: bool) {
        match self {
            Backend::Sqlite(adapter) => adapter.set_logger(enabled),
            Backend::Postgres(adapter) => adapter.set_logger(enabled),
            Backend::PgBouncer(adapter) => adapter.set_logger(enabled),
            Backend::Mysql(adapter) => adapter.set_logger(enabled),
        }
    }

    pub fn logger_enabled(&self) -> bool {
        match self {
            Backend::Sqlite(adapter) => adapter.logger_enabled(),
            Backend::Postgres(adapter) => adapter.logger_enabled(),
            Backend::PgBouncer(adapter) => adapter.logger_enabled(),
            Backend::Mysql(adapter) => adapter.logger_enabled(),
        }
    }

    fn log_query(&self, sql: &str, params: &[crate::DinocoValue]) {
        if self.logger_enabled() {
            println!("SQL: {}, params: {:#?}", sql, params);
        }
    }

    fn log_transaction(&self, commands: &[CompiledTransactionCommand]) {
        for command in commands {
            for statement in &command.statements {
                self.log_query(&statement.sql, &statement.params);
            }
        }
    }

    pub async fn execute_transaction(&self, commands: Vec<TransactionCommand>) -> anyhow::Result<TransactionResults> {
        match self {
            Backend::Sqlite(adapter) => {
                let commands =
                    commands.into_iter().map(|command| command.compile(adapter)).collect::<anyhow::Result<Vec<_>>>()?;
                self.log_transaction(&commands);
                adapter.execute_compiled_transaction(commands).await
            }
            Backend::Postgres(adapter) => {
                let commands =
                    commands.into_iter().map(|command| command.compile(adapter)).collect::<anyhow::Result<Vec<_>>>()?;
                self.log_transaction(&commands);
                adapter.execute_compiled_transaction(commands).await
            }
            Backend::PgBouncer(adapter) => {
                let commands =
                    commands.into_iter().map(|command| command.compile(adapter)).collect::<anyhow::Result<Vec<_>>>()?;
                self.log_transaction(&commands);
                adapter.inner().execute_compiled_transaction(commands).await
            }
            Backend::Mysql(adapter) => {
                if commands.iter().any(TransactionCommand::has_returning_write) {
                    anyhow::bail!(
                        "MySQL transaction batches do not support `.returning::<T>()` or `find_and_update::<T>()` yet."
                    );
                }
                let commands =
                    commands.into_iter().map(|command| command.compile(adapter)).collect::<anyhow::Result<Vec<_>>>()?;
                self.log_transaction(&commands);
                adapter.execute_compiled_transaction(commands).await
            }
        }
    }

    pub async fn query<M>(&self, query: FindQuery) -> anyhow::Result<Vec<M>>
    where
        M: DinocoRowModel,
    {
        match &self {
            Backend::Sqlite(adapter) => {
                let (sql, params) = adapter.compile_find_query(query);

                self.log_query(&sql, &params);

                adapter.query::<M>(&sql, &params).await
            }
            Backend::Postgres(adapter) => {
                let (sql, params) = adapter.compile_find_query(query);

                self.log_query(&sql, &params);

                adapter.query::<M>(&sql, &params).await
            }
            Backend::PgBouncer(adapter) => {
                let (sql, params) = adapter.compile_find_query(query);

                self.log_query(&sql, &params);

                adapter.query::<M>(&sql, &params).await
            }
            Backend::Mysql(adapter) => {
                let (sql, params) = adapter.compile_find_query(query);

                self.log_query(&sql, &params);

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

                self.log_query(&sql, &params);

                adapter.query_optional::<M>(&sql, &params).await
            }
            Backend::Postgres(adapter) => {
                let (sql, extra_params) = adapter.compile_relation_join_query(query);
                let mut params = params.to_vec();
                params.extend(extra_params);

                self.log_query(&sql, &params);

                adapter.query_optional::<M>(&sql, &params).await
            }
            Backend::PgBouncer(adapter) => {
                let (sql, extra_params) = adapter.compile_relation_join_query(query);
                let mut params = params.to_vec();
                params.extend(extra_params);

                self.log_query(&sql, &params);

                adapter.query_optional::<M>(&sql, &params).await
            }
            Backend::Mysql(adapter) => {
                let (sql, extra_params) = adapter.compile_relation_join_query(query);
                let mut params = params.to_vec();
                params.extend(extra_params);

                self.log_query(&sql, &params);

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

                self.log_query(&sql, &params);

                adapter.query::<M>(&sql, &params).await
            }
            Backend::Postgres(adapter) => {
                let (sql, params) = adapter.compile_relation_batch_query(query);

                self.log_query(&sql, &params);

                adapter.query::<M>(&sql, &params).await
            }
            Backend::PgBouncer(adapter) => {
                let (sql, params) = adapter.compile_relation_batch_query(query);

                self.log_query(&sql, &params);

                adapter.query::<M>(&sql, &params).await
            }
            Backend::Mysql(adapter) => {
                let (sql, params) = adapter.compile_relation_batch_query(query);

                self.log_query(&sql, &params);

                adapter.query::<M>(&sql, &params).await
            }
        }
    }

    pub async fn query_many_to_many_relation<M>(
        &self,
        query: ManyToManyRelationQuery,
        params: &[crate::DinocoValue],
    ) -> anyhow::Result<Vec<M>>
    where
        M: DinocoRowModel,
    {
        macro_rules! query {
            ($adapter:expr) => {{
                let (sql, extra_params) = $adapter.compile_many_to_many_relation_query(query);
                let mut params = params.to_vec();
                params.extend(extra_params);
                self.log_query(&sql, &params);
                $adapter.query::<M>(&sql, &params).await
            }};
        }

        match &self {
            Backend::Sqlite(adapter) => query!(adapter),
            Backend::Postgres(adapter) => query!(adapter),
            Backend::PgBouncer(adapter) => query!(adapter),
            Backend::Mysql(adapter) => query!(adapter),
        }
    }

    pub async fn insert(&self, query: InsertQuery) -> anyhow::Result<usize> {
        match &self {
            Backend::Sqlite(adapter) => {
                let (sql, params) = adapter.compile_insert_query(query);

                self.log_query(&sql, &params);

                adapter.execute(&sql, &params).await
            }
            Backend::Postgres(adapter) => {
                let (sql, params) = adapter.compile_insert_query(query);

                self.log_query(&sql, &params);

                adapter.execute(&sql, &params).await
            }
            Backend::PgBouncer(adapter) => {
                let (sql, params) = adapter.compile_insert_query(query);

                self.log_query(&sql, &params);

                adapter.execute(&sql, &params).await
            }
            Backend::Mysql(adapter) => {
                let (sql, params) = adapter.compile_insert_query(query);

                self.log_query(&sql, &params);

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

                self.log_query(&sql, &params);

                adapter.query::<M>(&sql, &params).await
            }
            Backend::Postgres(adapter) => {
                let (sql, params) = adapter.compile_insert_query(query);

                self.log_query(&sql, &params);

                adapter.query::<M>(&sql, &params).await
            }
            Backend::PgBouncer(adapter) => {
                let (sql, params) = adapter.compile_insert_query(query);

                self.log_query(&sql, &params);

                adapter.query::<M>(&sql, &params).await
            }
            Backend::Mysql(adapter) => {
                let Some(returning) = query.returning else {
                    let (sql, params) = adapter.compile_insert_query(query);

                    self.log_query(&sql, &params);

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

                self.log_query(&sql, &params);

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

                self.log_query(&sql, &params);

                adapter.query::<M>(&sql, &params).await
            }
        }
    }

    pub async fn update(&self, query: UpdateQuery) -> anyhow::Result<usize> {
        match &self {
            Backend::Sqlite(adapter) => {
                let (sql, params) = adapter.compile_update_query(query);

                self.log_query(&sql, &params);

                adapter.execute(&sql, &params).await
            }
            Backend::Postgres(adapter) => {
                let (sql, params) = adapter.compile_update_query(query);

                self.log_query(&sql, &params);

                adapter.execute(&sql, &params).await
            }
            Backend::PgBouncer(adapter) => {
                let (sql, params) = adapter.compile_update_query(query);

                self.log_query(&sql, &params);

                adapter.execute(&sql, &params).await
            }
            Backend::Mysql(adapter) => {
                let (sql, params) = adapter.compile_update_query(query);

                self.log_query(&sql, &params);

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

                self.log_query(&sql, &params);

                adapter.query::<M>(&sql, &params).await
            }
            Backend::Postgres(adapter) => {
                let (sql, params) = adapter.compile_update_query(query);

                self.log_query(&sql, &params);

                adapter.query::<M>(&sql, &params).await
            }
            Backend::PgBouncer(adapter) => {
                let (sql, params) = adapter.compile_update_query(query);

                self.log_query(&sql, &params);

                adapter.query::<M>(&sql, &params).await
            }
            Backend::Mysql(adapter) => {
                let Some(returning) = query.returning else {
                    let (sql, params) = adapter.compile_update_query(query);

                    self.log_query(&sql, &params);

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

                self.log_query(&sql, &params);

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

                self.log_query(&sql, &params);

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

                self.log_query(&sql, &params);

                adapter.query::<M>(&sql, &params).await
            }
        }
    }

    /// Executes the mutation before any MySQL compatibility read. This keeps
    /// arithmetic and WHERE predicates in one atomic UPDATE statement.
    pub async fn atomic_update_returning<M>(&self, query: UpdateQuery) -> anyhow::Result<Vec<M>>
    where
        M: DinocoRowModel,
    {
        if !matches!(self, Backend::Mysql(_)) {
            return self.update_returning::<M>(query).await;
        }

        let Backend::Mysql(adapter) = self else {
            unreachable!();
        };
        let returning =
            query.returning.ok_or_else(|| anyhow::anyhow!("atomic update returning requires a projection"))?;
        if query.sets.iter().any(|set| set.field == "id") {
            anyhow::bail!("MySQL atomic find_and_update cannot change the `id` field");
        }
        let find_conditions = query.post_update_reload_conditions();

        let table = query.table;
        let mut update = query;
        update.returning = None;
        let (update_sql, update_params) = adapter.compile_update_query(update);
        self.log_query(&update_sql, &update_params);

        let find = FindQuery {
            fields: returning,
            from: table,
            conditions: find_conditions,
            limit: 1,
            skip: -1,
            order_by: None,
        };
        let (find_sql, find_params) = adapter.compile_find_query(find);
        self.log_query(&find_sql, &find_params);
        adapter.execute_atomic_update_returning::<M>(&update_sql, &update_params, &find_sql, &find_params).await
    }

    pub async fn delete(&self, query: DeleteQuery) -> anyhow::Result<usize> {
        match &self {
            Backend::Sqlite(adapter) => {
                let (sql, params) = adapter.compile_delete_query(query);

                self.log_query(&sql, &params);

                adapter.execute(&sql, &params).await
            }
            Backend::Postgres(adapter) => {
                let (sql, params) = adapter.compile_delete_query(query);

                self.log_query(&sql, &params);

                adapter.execute(&sql, &params).await
            }
            Backend::PgBouncer(adapter) => {
                let (sql, params) = adapter.compile_delete_query(query);

                self.log_query(&sql, &params);

                adapter.execute(&sql, &params).await
            }
            Backend::Mysql(adapter) => {
                let (sql, params) = adapter.compile_delete_query(query);

                self.log_query(&sql, &params);

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

                self.log_query(&sql, &params);

                adapter.query::<M>(&sql, &params).await
            }
            Backend::Postgres(adapter) => {
                let (sql, params) = adapter.compile_delete_query(query);

                self.log_query(&sql, &params);

                adapter.query::<M>(&sql, &params).await
            }
            Backend::PgBouncer(adapter) => {
                let (sql, params) = adapter.compile_delete_query(query);

                self.log_query(&sql, &params);

                adapter.query::<M>(&sql, &params).await
            }
            Backend::Mysql(adapter) => {
                let Some(returning) = query.returning else {
                    let (sql, params) = adapter.compile_delete_query(query);

                    self.log_query(&sql, &params);

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

                self.log_query(&sql, &params);

                let rows = adapter.query::<M>(&sql, &params).await?;
                let mut delete_query = query;
                delete_query.returning = None;
                let (sql, params) = adapter.compile_delete_query(delete_query);

                self.log_query(&sql, &params);

                adapter.execute(&sql, &params).await?;

                Ok(rows)
            }
        }
    }

    pub async fn count(&self, query: CountQuery) -> anyhow::Result<i64> {
        match &self {
            Backend::Sqlite(adapter) => {
                let (sql, params) = adapter.compile_count_query(query);

                self.log_query(&sql, &params);

                adapter.query_count(&sql, &params).await
            }
            Backend::Postgres(adapter) => {
                let (sql, params) = adapter.compile_count_query(query);

                self.log_query(&sql, &params);

                adapter.query_count(&sql, &params).await
            }
            Backend::PgBouncer(adapter) => {
                let (sql, params) = adapter.compile_count_query(query);

                self.log_query(&sql, &params);

                adapter.query_count(&sql, &params).await
            }
            Backend::Mysql(adapter) => {
                let (sql, params) = adapter.compile_count_query(query);

                self.log_query(&sql, &params);

                adapter.query_count(&sql, &params).await
            }
        }
    }

    pub async fn count_relation(&self, query: RelationCountQuery) -> anyhow::Result<i64> {
        match &self {
            Backend::Sqlite(adapter) => {
                let (sql, params) = adapter.compile_relation_count_query(query);

                self.log_query(&sql, &params);

                adapter.query_count(&sql, &params).await
            }
            Backend::Postgres(adapter) => {
                let (sql, params) = adapter.compile_relation_count_query(query);

                self.log_query(&sql, &params);

                adapter.query_count(&sql, &params).await
            }
            Backend::PgBouncer(adapter) => {
                let (sql, params) = adapter.compile_relation_count_query(query);

                self.log_query(&sql, &params);

                adapter.query_count(&sql, &params).await
            }
            Backend::Mysql(adapter) => {
                let (sql, params) = adapter.compile_relation_count_query(query);

                self.log_query(&sql, &params);

                adapter.query_count(&sql, &params).await
            }
        }
    }

    pub async fn count_many_to_many_relation(&self, query: ManyToManyRelationCountQuery) -> anyhow::Result<i64> {
        macro_rules! count {
            ($adapter:expr) => {{
                let (sql, params) = $adapter.compile_many_to_many_relation_count_query(query);
                self.log_query(&sql, &params);
                $adapter.query_count(&sql, &params).await
            }};
        }

        match &self {
            Backend::Sqlite(adapter) => count!(adapter),
            Backend::Postgres(adapter) => count!(adapter),
            Backend::PgBouncer(adapter) => count!(adapter),
            Backend::Mysql(adapter) => count!(adapter),
        }
    }
}
