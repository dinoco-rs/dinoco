use std::sync::Arc;

use dinoco_engine::{
    DinocoClient, DinocoRowModel, DinocoValue, FindQuery, ManyToManyRelationQuery, RelationOccurrenceQuery,
    TransactionCommand, TransactionExecutor,
};

use crate::{Observer, RowCountKind, TransactionContext, active_transaction, observe_rows};

/// Where a read builder (`find_first`, `find_many`) runs: `&client`, which
/// may route to a read replica, or the `tx` handed to a transaction closure,
/// which reads on the transaction's own connection and sees its writes.
pub trait ReadExecutor: Send + Sync {
    #[doc(hidden)]
    fn read_target(&self, read_primary: bool) -> anyhow::Result<ReadTarget<'_>>;
}

#[doc(hidden)]
pub struct ReadTarget<'a> {
    inner: ReadTargetInner<'a>,
    observer: Option<Observer>,
}

enum ReadTargetInner<'a> {
    Client { client: &'a DinocoClient, read_primary: bool },
    Transaction(TransactionExecutor),
}

impl ReadTarget<'_> {
    pub(crate) async fn query<M>(&self, query: FindQuery) -> anyhow::Result<Vec<M>>
    where
        M: DinocoRowModel,
    {
        observe_rows(
            self.observer.as_ref(),
            RowCountKind::Find,
            query.from,
            query,
            |compiler, query| compiler.compile_find_query(query),
            |query| async move {
                match &self.inner {
                    ReadTargetInner::Client { client, read_primary } => {
                        client.read_backend(*read_primary).query::<M>(query).await
                    }
                    ReadTargetInner::Transaction(executor) => {
                        executor.execute(TransactionCommand::find_many::<M>(query)).await
                    }
                }
            },
            Vec::len,
        )
        .await
    }

    pub(crate) async fn query_relation_occurrences<M>(
        &self,
        query: RelationOccurrenceQuery,
        keys: &[DinocoValue],
    ) -> anyhow::Result<Vec<M>>
    where
        M: DinocoRowModel,
    {
        observe_rows(
            self.observer.as_ref(),
            RowCountKind::Find,
            query.query.from,
            (query, keys.to_vec()),
            |compiler, (query, mut keys)| {
                let (sql, params) = compiler.compile_relation_occurrence_query(query);
                keys.extend(params);
                (sql, keys)
            },
            |(query, keys)| async move {
                match &self.inner {
                    ReadTargetInner::Client { client, read_primary } => {
                        client.read_backend(*read_primary).query_relation_occurrences::<M>(query, &keys).await
                    }
                    ReadTargetInner::Transaction(executor) => {
                        executor.execute(TransactionCommand::relation_occurrences::<M>(query, keys)).await
                    }
                }
            },
            Vec::len,
        )
        .await
    }

    pub(crate) async fn query_many_to_many_relation<M>(
        &self,
        query: ManyToManyRelationQuery,
        keys: &[DinocoValue],
    ) -> anyhow::Result<Vec<M>>
    where
        M: DinocoRowModel,
    {
        observe_rows(
            self.observer.as_ref(),
            RowCountKind::Find,
            query.query.from,
            (query, keys.to_vec()),
            |compiler, (query, mut keys)| {
                let (sql, params) = compiler.compile_many_to_many_relation_query(query);
                keys.extend(params);
                (sql, keys)
            },
            |(query, keys)| async move {
                match &self.inner {
                    ReadTargetInner::Client { client, read_primary } => {
                        client.read_backend(*read_primary).query_many_to_many_relation::<M>(query, &keys).await
                    }
                    ReadTargetInner::Transaction(executor) => {
                        executor.execute(TransactionCommand::many_to_many_relation::<M>(query, keys)).await
                    }
                }
            },
            Vec::len,
        )
        .await
    }
}

impl ReadExecutor for DinocoClient {
    fn read_target(&self, read_primary: bool) -> anyhow::Result<ReadTarget<'_>> {
        Ok(ReadTarget {
            inner: ReadTargetInner::Client { client: self, read_primary },
            observer: Observer::for_client(self),
        })
    }
}

/// Inside a transaction every read goes to the primary connection that owns
/// the transaction, so `.read_in_primary()` has nothing left to change.
impl ReadExecutor for TransactionContext {
    fn read_target(&self, _read_primary: bool) -> anyhow::Result<ReadTarget<'_>> {
        let active = active_transaction()?;
        Ok(ReadTarget { inner: ReadTargetInner::Transaction(active.executor), observer: active.observer })
    }
}

impl<T> ReadExecutor for &T
where
    T: ReadExecutor + ?Sized,
{
    fn read_target(&self, read_primary: bool) -> anyhow::Result<ReadTarget<'_>> {
        (**self).read_target(read_primary)
    }
}

impl<T> ReadExecutor for Arc<T>
where
    T: ReadExecutor + ?Sized,
{
    fn read_target(&self, read_primary: bool) -> anyhow::Result<ReadTarget<'_>> {
        (**self).read_target(read_primary)
    }
}
