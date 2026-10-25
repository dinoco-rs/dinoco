use std::sync::Arc;

use dinoco_engine::{
    AggregateQuery, CountQuery, DinocoClient, DinocoRowModel, DinocoValue, ExistsQuery, FindQuery,
    ManyToManyRelationCountQuery, ManyToManyRelationQuery, RelationCountQuery, RelationOccurrenceQuery,
    TransactionCommand, TransactionExecutor,
};
use futures_util::future::Either;

use crate::{Observer, RowCountKind, TransactionContext, active_executor, active_transaction, observe_rows};

/// Where a read builder (`find_first`, `find_many`, `count`, `exists`) runs:
/// `&client`, which may route to a read replica, or the `tx` handed to a
/// transaction closure, which reads on the transaction's own connection and
/// sees its writes.
///
/// Transaction reads are awaited boxed, so the far more common client reads
/// do not carry the transaction channel's state in their futures.
pub trait ReadExecutor: Send + Sync {
    #[doc(hidden)]
    fn read_target(&self, read_primary: bool) -> anyhow::Result<ReadTarget<'_>>;

    /// The connection alone, for the reads that report to no hook.
    #[doc(hidden)]
    fn read_source(&self, read_primary: bool) -> anyhow::Result<ReadSource<'_>> {
        self.read_target(read_primary).map(|target| target.source)
    }
}

#[doc(hidden)]
pub struct ReadTarget<'a> {
    source: ReadSource<'a>,
    observer: Option<Observer>,
}

/// The connection a read runs on. `count` and `exists` report to no hook, so
/// they read through this alone, which keeps the observer out of their
/// futures and skips building it. Cloning it copies a reference or the
/// transaction's channel handle.
#[doc(hidden)]
#[derive(Clone)]
pub struct ReadSource<'a>(ReadConnection<'a>);

#[derive(Clone)]
enum ReadConnection<'a> {
    Client { client: &'a DinocoClient, read_primary: bool },
    Transaction(TransactionExecutor),
}

// Each read returns the future of its round trip (wrapped to report it to a
// hook, if any) instead of awaiting it, so the caller's future holds that one
// directly: an `async fn` here would keep another copy of the query beside it.
// The connection is picked as each read is built.
impl ReadTarget<'_> {
    pub(crate) fn query<M>(&self, query: FindQuery) -> impl Future<Output = anyhow::Result<Vec<M>>> + Send
    where
        M: DinocoRowModel,
    {
        observe_rows(
            self.observer.as_ref(),
            RowCountKind::Find,
            query.from,
            query,
            |compiler, query| compiler.compile_find_query(query),
            |query| match &self.source.0 {
                ReadConnection::Client { client, read_primary } => {
                    Either::Left(client.read_backend(*read_primary).query::<M>(query))
                }
                ReadConnection::Transaction(executor) => {
                    Either::Right(Box::pin(executor.execute(TransactionCommand::find_many::<M>(query))))
                }
            },
            Vec::len,
        )
    }

    /// Reported to `on_find` like any other read, with one row per group.
    pub(crate) fn aggregate<M>(&self, query: AggregateQuery) -> impl Future<Output = anyhow::Result<Vec<M>>> + Send
    where
        M: DinocoRowModel,
    {
        let zone = query.group.and_then(|group| group.key.time_zone());
        let rows = observe_rows(
            self.observer.as_ref(),
            RowCountKind::Find,
            query.source.from,
            query,
            |compiler, query| compiler.compile_aggregate_query(query),
            |query| match &self.source.0 {
                ReadConnection::Client { client, read_primary } => {
                    Either::Left(client.read_backend(*read_primary).aggregate::<M>(query))
                }
                ReadConnection::Transaction(executor) => {
                    Either::Right(Box::pin(executor.execute(TransactionCommand::aggregate::<M>(query))))
                }
            },
            Vec::len,
        );

        match zone {
            None => Either::Left(rows),
            // Rare: boxed, time zone check included, to keep it out of every
            // aggregate's future.
            Some(zone) => Either::Right(Box::pin(async move {
                self.require_time_zone(zone).await?;
                rows.await
            })),
        }
    }

    /// Fails when the database cannot compute dates in `zone`, instead of
    /// grouping every row under a `NULL` key: MySQL needs its time zone tables.
    async fn require_time_zone(&self, zone: dinoco_engine::chrono_tz::Tz) -> anyhow::Result<()> {
        let supported = match &self.source.0 {
            ReadConnection::Client { client, read_primary } => {
                client.read_backend(*read_primary).time_zone_supported(zone).await?
            }
            ReadConnection::Transaction(executor) if executor.is_mysql() => {
                executor.execute::<i64>(TransactionCommand::mysql_time_zone_check(zone)).await? != 0
            }
            ReadConnection::Transaction(_) => true,
        };
        if !supported {
            anyhow::bail!(
                "MySQL has no time zone data for `{}`: load its time zone tables with `mysql_tzinfo_to_sql` \
                 (https://dev.mysql.com/doc/refman/8.4/en/time-zone-support.html)",
                zone.name()
            );
        }
        Ok(())
    }

    /// `keys` are the parent keys, bound before the query's own values.
    pub(crate) fn query_relation_occurrences<M>(
        &self,
        query: RelationOccurrenceQuery,
        keys: Vec<DinocoValue>,
    ) -> impl Future<Output = anyhow::Result<Vec<M>>> + Send
    where
        M: DinocoRowModel,
    {
        observe_rows(
            self.observer.as_ref(),
            RowCountKind::Find,
            query.query.from,
            (query, keys),
            |compiler, (query, keys)| compiler.compile_relation_occurrence_query_with_keys(query, keys),
            |(query, keys)| match &self.source.0 {
                ReadConnection::Client { client, read_primary } => Either::Left(
                    client.read_backend(*read_primary).query_relation_occurrences_with_keys::<M>(query, keys),
                ),
                ReadConnection::Transaction(executor) => Either::Right(Box::pin(
                    executor.execute(TransactionCommand::relation_occurrences::<M>(query, keys)),
                )),
            },
            Vec::len,
        )
    }

    /// `keys` are the parent keys, bound before the query's own values.
    pub(crate) fn query_many_to_many_relation<M>(
        &self,
        query: ManyToManyRelationQuery,
        keys: Vec<DinocoValue>,
    ) -> impl Future<Output = anyhow::Result<Vec<M>>> + Send
    where
        M: DinocoRowModel,
    {
        observe_rows(
            self.observer.as_ref(),
            RowCountKind::Find,
            query.query.from,
            (query, keys),
            |compiler, (query, mut keys)| {
                let (sql, params) = compiler.compile_many_to_many_relation_query(query);
                keys.extend(params);
                (sql, keys)
            },
            |(query, keys)| match &self.source.0 {
                ReadConnection::Client { client, read_primary } => Either::Left(
                    client.read_backend(*read_primary).query_many_to_many_relation_with_keys::<M>(query, keys),
                ),
                ReadConnection::Transaction(executor) => Either::Right(Box::pin(
                    executor.execute(TransactionCommand::many_to_many_relation::<M>(query, keys)),
                )),
            },
            Vec::len,
        )
    }
}

// Each read consumes the source and returns the backend's future instead of
// awaiting it, so the builder's future holds that one directly: no source
// kept alive across the await, and no state machine (with another copy of
// the query) around it.
impl<'a> ReadSource<'a> {
    pub(crate) fn count(self, query: CountQuery) -> impl Future<Output = anyhow::Result<i64>> + Send + 'a {
        match self.0 {
            ReadConnection::Client { client, read_primary } => {
                Either::Left(client.read_backend(read_primary).count(query))
            }
            ReadConnection::Transaction(executor) => {
                Either::Right(Box::pin(async move { executor.execute(TransactionCommand::count(query)).await }))
            }
        }
    }

    pub(crate) fn exists(self, query: ExistsQuery) -> impl Future<Output = anyhow::Result<bool>> + Send + 'a {
        match self.0 {
            ReadConnection::Client { client, read_primary } => {
                Either::Left(client.read_backend(read_primary).exists(query))
            }
            ReadConnection::Transaction(executor) => {
                Either::Right(Box::pin(async move { executor.execute(TransactionCommand::exists(query)).await }))
            }
        }
    }

    pub(crate) fn count_relation(
        self,
        query: RelationCountQuery,
    ) -> impl Future<Output = anyhow::Result<i64>> + Send + 'a {
        match self.0 {
            ReadConnection::Client { client, read_primary } => {
                Either::Left(client.read_backend(read_primary).count_relation(query))
            }
            ReadConnection::Transaction(executor) => {
                Either::Right(Box::pin(
                    async move { executor.execute(TransactionCommand::relation_count(query)).await },
                ))
            }
        }
    }

    pub(crate) fn count_many_to_many_relation(
        self,
        query: ManyToManyRelationCountQuery,
    ) -> impl Future<Output = anyhow::Result<i64>> + Send + 'a {
        match self.0 {
            ReadConnection::Client { client, read_primary } => {
                Either::Left(client.read_backend(read_primary).count_many_to_many_relation(query))
            }
            ReadConnection::Transaction(executor) => Either::Right(Box::pin(async move {
                executor.execute(TransactionCommand::many_to_many_relation_count(query)).await
            })),
        }
    }
}

impl ReadExecutor for DinocoClient {
    fn read_target(&self, read_primary: bool) -> anyhow::Result<ReadTarget<'_>> {
        Ok(ReadTarget { source: self.read_source(read_primary)?, observer: Observer::for_client(self) })
    }

    fn read_source(&self, read_primary: bool) -> anyhow::Result<ReadSource<'_>> {
        Ok(ReadSource(ReadConnection::Client { client: self, read_primary }))
    }
}

/// Inside a transaction every read goes to the primary connection that owns
/// the transaction, so `.read_in_primary()` has nothing left to change.
impl ReadExecutor for TransactionContext {
    fn read_target(&self, _read_primary: bool) -> anyhow::Result<ReadTarget<'_>> {
        let active = active_transaction()?;
        Ok(ReadTarget { source: ReadSource(ReadConnection::Transaction(active.executor)), observer: active.observer })
    }

    fn read_source(&self, _read_primary: bool) -> anyhow::Result<ReadSource<'_>> {
        Ok(ReadSource(ReadConnection::Transaction(active_executor()?)))
    }
}

impl<T> ReadExecutor for &T
where
    T: ReadExecutor + ?Sized,
{
    fn read_target(&self, read_primary: bool) -> anyhow::Result<ReadTarget<'_>> {
        (**self).read_target(read_primary)
    }

    fn read_source(&self, read_primary: bool) -> anyhow::Result<ReadSource<'_>> {
        (**self).read_source(read_primary)
    }
}

impl<T> ReadExecutor for Arc<T>
where
    T: ReadExecutor + ?Sized,
{
    fn read_target(&self, read_primary: bool) -> anyhow::Result<ReadTarget<'_>> {
        (**self).read_target(read_primary)
    }

    fn read_source(&self, read_primary: bool) -> anyhow::Result<ReadSource<'_>> {
        (**self).read_source(read_primary)
    }
}
