#[cfg(feature = "mysql")]
mod mysql;
#[cfg(feature = "postgres")]
mod postgres;
pub(crate) mod sql;
#[cfg(feature = "sqlite")]
mod sqlite;

#[cfg(feature = "mysql")]
pub use mysql::*;
#[cfg(feature = "postgres")]
pub use postgres::*;
#[cfg(feature = "sqlite")]
pub use sqlite::*;

use sql::Dialect;

use crate::{
    AggregateQuery, CountQuery, DeleteQuery, DinocoRowModel, DinocoSqlCompiler, DinocoValue, ExistsQuery, FindQuery,
    FindWhere, InsertQuery, LookupQuery, ManyToManyRelationCountQuery, ManyToManyRelationQuery, OrderByList,
    RelationBatchQuery, RelationCountQuery, RelationJoinQuery, RelationOccurrenceQuery, UpdateQuery,
};

/// Cloning a backend is cheap: adapters share their connection pool.
#[derive(Clone)]
pub enum Backend {
    #[cfg(feature = "sqlite")]
    Sqlite(SqliteAdapter),
    #[cfg(feature = "postgres")]
    Postgres(PostgresAdapter),
    #[cfg(feature = "postgres")]
    PgBouncer(PgBouncerAdapter),
    #[cfg(feature = "mysql")]
    Mysql(MySqlAdapter),
}

/// What an adapter does with a row the model cannot be decoded from.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Decode {
    /// Fail the whole query.
    Strict,
    /// Leave the row out; used by `LEFT JOIN` relation loads, whose
    /// unmatched rows carry only NULLs.
    Lenient,
}

type Compiled = (String, Vec<DinocoValue>);

/// Runs `$body` with `$adapter` bound to the adapter of `$backend`. PgBouncer
/// executes through the PostgreSQL adapter it wraps.
///
/// Statement executions are awaited through [`boxed`]: a pool checkout plus a
/// round trip makes for a large state machine (up to ~2 KB for PostgreSQL),
/// and keeping it out of line keeps every builder future that awaits one
/// small, for one allocation per round trip.
macro_rules! with_adapter {
    ($backend:expr, $adapter:ident => $body:expr) => {
        match $backend {
            #[cfg(feature = "sqlite")]
            Backend::Sqlite($adapter) => $body,
            #[cfg(feature = "postgres")]
            Backend::Postgres($adapter) => $body,
            #[cfg(feature = "postgres")]
            Backend::PgBouncer(adapter) => {
                let $adapter = adapter.inner();
                $body
            }
            #[cfg(feature = "mysql")]
            Backend::Mysql($adapter) => $body,
        }
    };
}

impl Backend {
    pub(crate) fn dialect(&self) -> Dialect {
        match self {
            #[cfg(feature = "sqlite")]
            Backend::Sqlite(_) => Dialect::Sqlite,
            #[cfg(feature = "postgres")]
            Backend::Postgres(_) | Backend::PgBouncer(_) => Dialect::Postgres,
            #[cfg(feature = "mysql")]
            Backend::Mysql(_) => Dialect::Mysql,
        }
    }

    fn is_mysql(&self) -> bool {
        #[cfg(feature = "mysql")]
        return matches!(self, Backend::Mysql(_));
        #[cfg(not(feature = "mysql"))]
        return false;
    }

    pub async fn begin_transaction(&self) -> anyhow::Result<crate::TransactionExecutor> {
        let sender = with_adapter!(self, adapter => boxed(adapter.begin_live_transaction()).await?);
        Ok(crate::TransactionExecutor { sender, mysql: self.is_mysql() })
    }

    /// The SQL compiler of this backend's database.
    pub fn sql_compiler(&self) -> &dyn DinocoSqlCompiler {
        match self {
            #[cfg(feature = "sqlite")]
            Backend::Sqlite(adapter) => adapter,
            #[cfg(feature = "postgres")]
            Backend::Postgres(adapter) => adapter,
            #[cfg(feature = "postgres")]
            Backend::PgBouncer(adapter) => adapter,
            #[cfg(feature = "mysql")]
            Backend::Mysql(adapter) => adapter,
        }
    }

    pub fn set_logger(&mut self, enabled: bool) {
        match self {
            #[cfg(feature = "sqlite")]
            Backend::Sqlite(adapter) => adapter.set_logger(enabled),
            #[cfg(feature = "postgres")]
            Backend::Postgres(adapter) => adapter.set_logger(enabled),
            #[cfg(feature = "postgres")]
            Backend::PgBouncer(adapter) => adapter.set_logger(enabled),
            #[cfg(feature = "mysql")]
            Backend::Mysql(adapter) => adapter.set_logger(enabled),
        }
    }

    pub fn logger_enabled(&self) -> bool {
        with_adapter!(self, adapter => adapter.logger_enabled())
    }

    fn log_query(&self, sql: &str, params: &[DinocoValue]) {
        if self.logger_enabled() {
            println!("SQL: {}, params: {:#?}", sql, params);
        }
    }

    async fn fetch<M>(&self, (sql, params): Compiled, decode: Decode) -> anyhow::Result<Vec<M>>
    where
        M: DinocoRowModel,
    {
        self.log_query(&sql, &params);
        with_adapter!(self, adapter => boxed(adapter.fetch(sql, params, decode)).await)
    }

    async fn run(&self, (sql, params): Compiled) -> anyhow::Result<usize> {
        self.log_query(&sql, &params);
        with_adapter!(self, adapter => boxed(adapter.run(sql, params)).await)
    }

    async fn fetch_count(&self, (sql, params): Compiled) -> anyhow::Result<i64> {
        self.log_query(&sql, &params);
        with_adapter!(self, adapter => boxed(adapter.fetch_count(sql, params)).await)
    }

    pub async fn query<M>(&self, query: FindQuery) -> anyhow::Result<Vec<M>>
    where
        M: DinocoRowModel,
    {
        self.fetch(sql::find(self.dialect(), query), Decode::Strict).await
    }

    /// `params` are the parent keys, bound before the query's own values.
    pub async fn query_relation_join_optional<M>(
        &self,
        query: RelationJoinQuery,
        params: &[DinocoValue],
    ) -> anyhow::Result<Vec<M>>
    where
        M: DinocoRowModel,
    {
        let (sql, extra_params) = sql::relation_join(self.dialect(), query);
        let mut all_params = Vec::with_capacity(params.len() + extra_params.len());
        all_params.extend_from_slice(params);
        all_params.extend(extra_params);

        self.fetch((sql, all_params), Decode::Lenient).await
    }

    pub async fn query_relation_batch<M>(&self, query: RelationBatchQuery) -> anyhow::Result<Vec<M>>
    where
        M: DinocoRowModel,
    {
        self.fetch(sql::relation_batch(self.dialect(), query), Decode::Strict).await
    }

    /// `params` are the parent keys, bound before the query's own values.
    #[doc(hidden)]
    pub async fn query_relation_occurrences<M>(
        &self,
        query: RelationOccurrenceQuery,
        params: &[DinocoValue],
    ) -> anyhow::Result<Vec<M>>
    where
        M: DinocoRowModel,
    {
        self.query_relation_occurrences_with_keys(query, params.to_vec()).await
    }

    /// [`query_relation_occurrences`](Self::query_relation_occurrences) taking
    /// ownership of the parent keys.
    #[doc(hidden)]
    pub async fn query_relation_occurrences_with_keys<M>(
        &self,
        query: RelationOccurrenceQuery,
        keys: Vec<DinocoValue>,
    ) -> anyhow::Result<Vec<M>>
    where
        M: DinocoRowModel,
    {
        self.fetch(sql::relation_occurrence_with_keys(self.dialect(), query, keys), Decode::Strict).await
    }

    /// `params` are the parent keys, bound before the query's own values.
    pub async fn query_many_to_many_relation<M>(
        &self,
        query: ManyToManyRelationQuery,
        params: &[DinocoValue],
    ) -> anyhow::Result<Vec<M>>
    where
        M: DinocoRowModel,
    {
        self.query_many_to_many_relation_with_keys(query, params.to_vec()).await
    }

    /// [`query_many_to_many_relation`](Self::query_many_to_many_relation)
    /// taking ownership of the parent keys.
    #[doc(hidden)]
    pub async fn query_many_to_many_relation_with_keys<M>(
        &self,
        query: ManyToManyRelationQuery,
        keys: Vec<DinocoValue>,
    ) -> anyhow::Result<Vec<M>>
    where
        M: DinocoRowModel,
    {
        self.fetch(sql::many_to_many_relation_with_keys(self.dialect(), query, keys), Decode::Strict).await
    }

    /// Whether the database can compute dates in `zone`. Only MySQL can lack
    /// it: it reads named time zones from tables that may not be loaded.
    #[doc(hidden)]
    pub async fn time_zone_supported(&self, zone: chrono_tz::Tz) -> anyhow::Result<bool> {
        #[cfg(feature = "mysql")]
        if let Backend::Mysql(adapter) = self {
            return adapter.time_zone_supported(zone).await;
        }
        let _ = zone;
        Ok(true)
    }

    /// One row of `M` for the whole aggregate, or one per group.
    pub async fn aggregate<M>(&self, query: AggregateQuery) -> anyhow::Result<Vec<M>>
    where
        M: DinocoRowModel,
    {
        self.fetch(sql::aggregate(self.dialect(), query), Decode::Strict).await
    }

    /// The row each key tuple names, in the order of the keys.
    pub async fn lookup<M>(&self, query: LookupQuery) -> anyhow::Result<Vec<M>>
    where
        M: DinocoRowModel,
    {
        self.fetch(sql::lookup(self.dialect(), query), Decode::Strict).await
    }

    pub async fn insert(&self, query: InsertQuery) -> anyhow::Result<usize> {
        self.run(sql::insert(self.dialect(), query)).await
    }

    pub async fn insert_returning<M>(&self, mut query: InsertQuery) -> anyhow::Result<Vec<M>>
    where
        M: DinocoRowModel,
    {
        // MySQL has no `RETURNING`: insert, then read the rows back by `id`.
        let (true, Some(returning)) = (self.is_mysql(), query.returning) else {
            return self.fetch(sql::insert(self.dialect(), query), Decode::Strict).await;
        };

        let id_index = query
            .fields
            .iter()
            .position(|field| *field == "id")
            .ok_or_else(|| anyhow::anyhow!("MySQL insert returning fallback requires an `id` field."))?;
        let ids = query.rows.iter().map(|row| row[id_index].clone()).collect::<Vec<_>>();
        let table = query.table;
        query.returning = None;
        self.run(sql::insert(Dialect::Mysql, query)).await?;

        self.query(reload_by_id(returning, table, ids)).await
    }

    pub async fn update(&self, query: UpdateQuery) -> anyhow::Result<usize> {
        self.run(sql::update(self.dialect(), query)).await
    }

    pub async fn update_returning<M>(&self, mut query: UpdateQuery) -> anyhow::Result<Vec<M>>
    where
        M: DinocoRowModel,
    {
        // MySQL has no `RETURNING`: find the ids, update, then read them back.
        let (true, Some(returning)) = (self.is_mysql(), query.returning) else {
            return self.fetch(sql::update(self.dialect(), query), Decode::Strict).await;
        };

        let ids = self
            .query::<crate::SingleIdRow>(FindQuery {
                fields: &["id"],
                from: query.table,
                conditions: query.conditions.clone(),
                limit: -1,
                skip: -1,
                order_by: OrderByList::None,
            })
            .await?
            .into_iter()
            .map(|row| row.id)
            .collect::<Vec<_>>();

        let table = query.table;
        query.returning = None;
        self.run(sql::update(Dialect::Mysql, query)).await?;

        if ids.is_empty() {
            return Ok(Vec::new());
        }

        self.query(reload_by_id(returning, table, ids)).await
    }

    /// Executes the mutation before any MySQL compatibility read. This keeps
    /// arithmetic and WHERE predicates in one atomic UPDATE statement.
    pub async fn atomic_update_returning<M>(&self, query: UpdateQuery) -> anyhow::Result<Vec<M>>
    where
        M: DinocoRowModel,
    {
        #[cfg(feature = "mysql")]
        #[allow(irrefutable_let_patterns)]
        if let Backend::Mysql(adapter) = self {
            let (update, find) = crate::transaction::mysql_atomic_update_statements(query)?;
            self.log_query(&update.0, &update.1);
            self.log_query(&find.0, &find.1);
            return boxed(adapter.execute_atomic_update_returning::<M>(update, find)).await;
        }

        self.update_returning::<M>(query).await
    }

    pub async fn delete(&self, query: DeleteQuery) -> anyhow::Result<usize> {
        self.run(sql::delete(self.dialect(), query)).await
    }

    pub async fn delete_returning<M>(&self, mut query: DeleteQuery) -> anyhow::Result<Vec<M>>
    where
        M: DinocoRowModel,
    {
        // MySQL has no `RETURNING`: read the rows, then delete them.
        let (true, Some(returning)) = (self.is_mysql(), query.returning) else {
            return self.fetch(sql::delete(self.dialect(), query), Decode::Strict).await;
        };

        let rows = self
            .query::<M>(FindQuery {
                fields: returning,
                from: query.table,
                conditions: query.conditions.clone(),
                limit: -1,
                skip: -1,
                order_by: OrderByList::None,
            })
            .await?;
        query.returning = None;
        self.run(sql::delete(Dialect::Mysql, query)).await?;

        Ok(rows)
    }

    pub async fn count(&self, query: CountQuery) -> anyhow::Result<i64> {
        self.fetch_count(sql::count(self.dialect(), query)).await
    }

    pub async fn exists(&self, query: ExistsQuery) -> anyhow::Result<bool> {
        let (sql, params) = sql::exists(self.dialect(), query);
        self.log_query(&sql, &params);
        with_adapter!(self, adapter => boxed(adapter.fetch_exists(sql, params)).await)
    }

    pub async fn count_relation(&self, query: RelationCountQuery) -> anyhow::Result<i64> {
        self.fetch_count(sql::relation_count(self.dialect(), query)).await
    }

    pub async fn count_many_to_many_relation(&self, query: ManyToManyRelationCountQuery) -> anyhow::Result<i64> {
        self.fetch_count(sql::many_to_many_relation_count(self.dialect(), query)).await
    }
}

fn boxed<'a, T>(
    future: impl std::future::Future<Output = T> + Send + 'a,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = T> + Send + 'a>> {
    Box::pin(future)
}

/// The rows with the given `id`s, projected onto `fields`.
fn reload_by_id(fields: &'static [&'static str], table: &'static str, ids: Vec<DinocoValue>) -> FindQuery {
    FindQuery {
        fields,
        from: table,
        conditions: vec![FindWhere::Batch("id", ids)],
        limit: -1,
        skip: -1,
        order_by: OrderByList::None,
    }
}
