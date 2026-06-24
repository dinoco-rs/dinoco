use std::marker::PhantomData;

use chrono::{DateTime, Utc};

use dinoco_engine::{DinocoAdapter, DinocoClient, DinocoError, DinocoResult};

use crate::{
    InsertConnection, InsertModel, InsertPayload, InsertRelation, IntoInsertPayloadOwned, Projection,
    execute_insert_payload_returning,
    execution::execute_reload_many_by_identity,
    queue::{QueueDispatch, dispatch_insert_lookup, enqueue_many_conditions},
};

mod connection;
mod relation;

#[derive(Debug, Clone)]
pub struct InsertMany<M, V = M> {
    items: Vec<V>,
    queue: Option<QueueDispatch>,
    marker: PhantomData<fn() -> M>,
}

#[derive(Debug, Clone)]
pub struct InsertManyWithRelation<M, R> {
    items: Vec<M>,
    related_items: Vec<R>,
}

#[derive(Debug, Clone)]
pub struct InsertManyWithRelations<M, R> {
    items: Vec<M>,
    related_groups: Vec<Vec<R>>,
}

#[derive(Debug, Clone)]
pub struct InsertManyWithConnections<M, R> {
    items: Vec<M>,
    connected_groups: Vec<Vec<R>>,
}

#[derive(Debug, Clone)]
pub struct InsertManyWithConnection<M, R> {
    items: Vec<M>,
    connected_items: Vec<R>,
}

#[derive(Debug, Clone)]
pub struct InsertManyReturning<M, V = M, S = M> {
    items: Vec<V>,
    queue: Option<QueueDispatch>,
    marker: PhantomData<fn() -> (M, S)>,
}

#[derive(Debug, Clone)]
pub struct InsertManyWithRelationReturning<M, R, S> {
    items: Vec<M>,
    related_items: Vec<R>,
    marker: PhantomData<fn() -> S>,
}

#[derive(Debug, Clone)]
pub struct InsertManyWithRelationsReturning<M, R, S> {
    items: Vec<M>,
    related_groups: Vec<Vec<R>>,
    marker: PhantomData<fn() -> S>,
}

#[derive(Debug, Clone)]
pub struct InsertManyWithConnectionsReturning<M, R, S> {
    items: Vec<M>,
    connected_groups: Vec<Vec<R>>,
    marker: PhantomData<fn() -> S>,
}

#[derive(Debug, Clone)]
pub struct InsertManyWithConnectionReturning<M, R, S> {
    items: Vec<M>,
    connected_items: Vec<R>,
    marker: PhantomData<fn() -> S>,
}

pub fn insert_many<M>() -> InsertMany<M>
where
    M: InsertModel,
{
    InsertMany { items: Vec::new(), queue: None, marker: PhantomData }
}

pub(super) fn validate_pair_len(left: usize, right: usize, context: &str) -> DinocoResult<()> {
    if left == right {
        return Ok(());
    }

    Err(DinocoError::ParseError(format!("insert_many {context} mismatch: {left} parent items for {right} entries")))
}

impl<M, V> InsertMany<M, V>
where
    M: InsertModel,
    V: InsertPayload<M>,
{
    pub fn values<N, I>(self, items: Vec<I>) -> InsertMany<M, N>
    where
        N: InsertPayload<M>,
        I: IntoInsertPayloadOwned<M, N>,
    {
        let items = items.into_iter().map(IntoInsertPayloadOwned::into_insert_payload_owned).collect::<Vec<_>>();

        InsertMany { items, queue: self.queue, marker: PhantomData }
    }

    pub fn returning(self) -> InsertManyReturning<M, V, M>
    where
        M: Projection<M>,
    {
        self.returning_as::<M>()
    }

    pub fn returning_as<S>(self) -> InsertManyReturning<M, V, S>
    where
        S: Projection<M>,
    {
        InsertManyReturning { items: self.items, queue: self.queue, marker: PhantomData }
    }

    #[doc(hidden)]
    pub fn __enqueue(mut self, event: impl Into<String>) -> Self {
        self.queue = Some(QueueDispatch::immediate(event));

        self
    }

    #[doc(hidden)]
    pub fn __enqueue_in(mut self, event: impl Into<String>, delay_ms: u64) -> Self {
        self.queue = Some(QueueDispatch::in_milliseconds(event, delay_ms));

        self
    }

    #[doc(hidden)]
    pub fn __enqueue_at(mut self, event: impl Into<String>, execute_at: DateTime<Utc>) -> Self {
        self.queue = Some(QueueDispatch::at(event, execute_at));

        self
    }

    pub fn execute<'a, A>(
        self,
        client: &'a DinocoClient<A>,
    ) -> impl std::future::Future<Output = DinocoResult<()>> + Send + 'a
    where
        M: Projection<M> + Send + Sync + 'a,
        V: Send + 'a,
        <V as InsertPayload<M>>::Nested: Send + 'a,
        A: DinocoAdapter,
    {
        async move {
            let queue = self.queue;
            let inserted = execute_insert_payload_returning::<M, V, M, A>(self.items, client).await?;

            if let Some(queue) = &queue {
                let conditions = inserted.iter().map(dispatch_insert_lookup).collect::<Vec<_>>();
                enqueue_many_conditions(client, queue, conditions).await?;
            }

            Ok(())
        }
    }
}

impl<M> InsertMany<M>
where
    M: InsertModel,
{
    pub fn with_relation<R>(self, related_items: Vec<R>) -> InsertManyWithRelation<M, R>
    where
        M: InsertRelation<R>,
        R: InsertModel,
    {
        InsertManyWithRelation { items: self.items, related_items }
    }

    pub fn with_relations<R>(self, related_groups: Vec<Vec<R>>) -> InsertManyWithRelations<M, R>
    where
        M: InsertRelation<R>,
        R: InsertModel,
    {
        InsertManyWithRelations { items: self.items, related_groups }
    }

    pub fn with_connections<R>(self, connected_groups: Vec<Vec<R>>) -> InsertManyWithConnections<M, R>
    where
        M: InsertConnection<R>,
    {
        InsertManyWithConnections { items: self.items, connected_groups }
    }

    pub fn with_connection<R>(self, connected_items: Vec<R>) -> InsertManyWithConnection<M, R>
    where
        M: InsertConnection<R>,
    {
        InsertManyWithConnection { items: self.items, connected_items }
    }
}

impl<M, V, S> InsertManyReturning<M, V, S>
where
    M: InsertModel,
    V: InsertPayload<M>,
    S: Projection<M>,
{
    #[doc(hidden)]
    pub fn __enqueue(mut self, event: impl Into<String>) -> Self {
        self.queue = Some(QueueDispatch::immediate(event));

        self
    }

    #[doc(hidden)]
    pub fn __enqueue_in(mut self, event: impl Into<String>, delay_ms: u64) -> Self {
        self.queue = Some(QueueDispatch::in_milliseconds(event, delay_ms));

        self
    }

    #[doc(hidden)]
    pub fn __enqueue_at(mut self, event: impl Into<String>, execute_at: DateTime<Utc>) -> Self {
        self.queue = Some(QueueDispatch::at(event, execute_at));

        self
    }

    pub fn execute<'a, A>(
        self,
        client: &'a DinocoClient<A>,
    ) -> impl std::future::Future<Output = DinocoResult<Vec<S>>> + Send + 'a
    where
        M: Projection<M> + Send + Sync + 'a,
        V: Send + 'a,
        <V as InsertPayload<M>>::Nested: Send + 'a,
        S: Send + Sync + 'a,
        A: DinocoAdapter,
    {
        async move {
            let inserted = execute_insert_payload_returning::<M, V, M, A>(self.items, client).await?;

            if let Some(queue) = &self.queue {
                let conditions = inserted.iter().map(dispatch_insert_lookup).collect::<Vec<_>>();
                enqueue_many_conditions(client, queue, conditions).await?;
            }

            execute_reload_many_by_identity::<M, S, A>(&inserted, client).await
        }
    }
}
