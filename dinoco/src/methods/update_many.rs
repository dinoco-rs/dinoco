use std::marker::PhantomData;

use chrono::{DateTime, Utc};

use dinoco_engine::{DinocoAdapter, DinocoClient, Expression};

use crate::{
    FieldUpdate, FindAndUpdateModel, Projection, UpdateModel, execute_update_fields, execute_update_fields_returning,
    queue::{QueueDispatch, enqueue_many_conditions},
};

#[derive(Debug, Clone)]
pub struct UpdateMany<M> {
    conditions: Vec<Expression>,
    updates: Vec<FieldUpdate>,
    queue: Option<QueueDispatch>,
    marker: PhantomData<fn() -> M>,
}

#[derive(Debug, Clone)]
pub struct UpdateManyReturning<M, S = M> {
    conditions: Vec<Expression>,
    updates: Vec<FieldUpdate>,
    queue: Option<QueueDispatch>,
    marker: PhantomData<fn() -> (M, S)>,
}

pub fn update_many<M>() -> UpdateMany<M>
where
    M: UpdateModel,
{
    UpdateMany { conditions: Vec::new(), updates: Vec::new(), queue: None, marker: PhantomData }
}

impl<M> UpdateMany<M>
where
    M: UpdateModel,
{
    pub fn cond<F>(mut self, closure: F) -> Self
    where
        F: FnOnce(M::Where) -> Expression,
    {
        self.conditions.push(closure(M::Where::default()));

        self
    }

    pub fn update<F>(mut self, closure: F) -> Self
    where
        M: FindAndUpdateModel,
        F: FnOnce(M::Update) -> FieldUpdate,
    {
        self.updates.push(closure(M::Update::default()));

        self
    }

    pub fn returning(self) -> UpdateManyReturning<M, M>
    where
        M: Projection<M>,
    {
        self.returning_as::<M>()
    }

    pub fn returning_as<S>(self) -> UpdateManyReturning<M, S>
    where
        M: Projection<M>,
        S: Projection<M>,
    {
        UpdateManyReturning {
            conditions: self.conditions,
            updates: self.updates,
            queue: self.queue,
            marker: PhantomData,
        }
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
        A: DinocoAdapter,
    {
        async move {
            let conditions = self.conditions;
            execute_update_fields::<M, A>(conditions.clone(), self.updates, client).await?;

            if let Some(queue) = &self.queue {
                enqueue_many_conditions(client, queue, vec![conditions]).await?;
            }

            Ok(())
        }
    }
}

impl<M, S> UpdateManyReturning<M, S>
where
    M: UpdateModel + Projection<M>,
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
        S: Send + Sync + 'a,
        A: DinocoAdapter,
    {
        async move {
            let conditions = self.conditions;
            let result = execute_update_fields_returning::<M, S, A>(conditions.clone(), self.updates, client).await?;

            if let Some(queue) = &self.queue {
                enqueue_many_conditions(client, queue, vec![conditions]).await?;
            }

            Ok(result)
        }
    }
}
