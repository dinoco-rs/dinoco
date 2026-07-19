use std::marker::PhantomData;

use chrono::{DateTime, Utc};

use dinoco_engine::{DinocoAdapter, DinocoClient, Expression};

use crate::{
    IntoUpdatePayloadOwned, Projection, UpdateModel, UpdatePayload, execute_update_many, execute_update_many_returning,
    queue::{QueueDispatch, dispatch_update_lookup, enqueue_many_conditions},
};

#[derive(Debug, Clone)]
pub struct UpdateMany<M, V = M> {
    conditions: Vec<Expression>,
    items: Vec<V>,
    queue: Option<QueueDispatch>,
    marker: PhantomData<fn() -> M>,
}

#[derive(Debug, Clone)]
pub struct UpdateManyReturning<M, V = M, S = M> {
    conditions: Vec<Expression>,
    items: Vec<V>,
    queue: Option<QueueDispatch>,
    marker: PhantomData<fn() -> (M, S)>,
}

pub fn update_many<M>() -> UpdateMany<M>
where
    M: UpdateModel,
{
    UpdateMany { conditions: Vec::new(), items: Vec::new(), queue: None, marker: PhantomData }
}

impl<M, V> UpdateMany<M, V>
where
    M: UpdateModel,
    V: UpdatePayload<M>,
{
    pub fn cond<F>(mut self, closure: F) -> Self
    where
        F: FnOnce(M::Where) -> Expression,
    {
        self.conditions.push(closure(M::Where::default()));

        self
    }

    pub fn values<N, I>(self, items: Vec<I>) -> UpdateMany<M, N>
    where
        N: UpdatePayload<M>,
        I: IntoUpdatePayloadOwned<M, N>,
    {
        let items = items.into_iter().map(IntoUpdatePayloadOwned::into_update_payload_owned).collect::<Vec<_>>();

        UpdateMany { conditions: self.conditions, items, queue: self.queue, marker: PhantomData }
    }

    pub fn returning<S>(self) -> UpdateManyReturning<M, V, S>
    where
        S: Projection<M>,
    {
        UpdateManyReturning { conditions: self.conditions, items: self.items, queue: self.queue, marker: PhantomData }
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
    ) -> impl std::future::Future<Output = dinoco_engine::DinocoResult<()>> + Send + 'a
    where
        M: Send + Sync + 'a,
        V: Send + Sync + 'a,
        A: DinocoAdapter,
    {
        async move {
            let queue_lookup = if self.queue.is_some() {
                Some(
                    self.items
                        .iter()
                        .map(|item| dispatch_update_lookup::<M, _>(item, &self.conditions))
                        .collect::<Vec<_>>(),
                )
            } else {
                None
            };

            execute_update_many::<M, V, A>(self.items, self.conditions, client).await?;

            if let (Some(queue), Some(queue_lookup)) = (&self.queue, queue_lookup) {
                enqueue_many_conditions(client, queue, queue_lookup).await?;
            }

            Ok(())
        }
    }
}

impl<M, V, S> UpdateManyReturning<M, V, S>
where
    M: UpdateModel,
    V: UpdatePayload<M>,
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
    ) -> impl std::future::Future<Output = dinoco_engine::DinocoResult<Vec<S>>> + Send + 'a
    where
        M: Send + Sync + 'a,
        V: Send + Sync + 'a,
        S: Send + Sync + 'a,
        A: DinocoAdapter,
    {
        async move {
            let queue_lookup = if self.queue.is_some() {
                Some(
                    self.items
                        .iter()
                        .map(|item| dispatch_update_lookup::<M, _>(item, &self.conditions))
                        .collect::<Vec<_>>(),
                )
            } else {
                None
            };
            let result = execute_update_many_returning::<M, V, S, A>(self.items, self.conditions, client).await?;

            if let (Some(queue), Some(queue_lookup)) = (&self.queue, queue_lookup) {
                enqueue_many_conditions(client, queue, queue_lookup).await?;
            }

            Ok(result)
        }
    }
}
