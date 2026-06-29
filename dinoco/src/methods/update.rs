use std::marker::PhantomData;

use chrono::{DateTime, Utc};

use dinoco_engine::{DinocoAdapter, DinocoClient, Expression};

use crate::{
    FieldUpdate, FindAndUpdateModel, Projection, RelationMutationModel, RelationMutationTarget, RelationWriteAction,
    RelationWritePlan, UpdateModel, execute_relation_writes, execute_update_fields, execute_update_fields_returning,
    queue::{QueueDispatch, enqueue_many_conditions, enqueue_single_conditions},
};

#[derive(Debug, Clone)]
pub struct MissingCondition;

#[derive(Debug, Clone)]
pub struct HasCondition;

#[derive(Debug, Clone)]
pub struct Update<M, Condition = MissingCondition> {
    conditions: Vec<Expression>,
    updates: Vec<FieldUpdate>,
    relation_writes: Vec<(RelationWriteAction, RelationWritePlan)>,
    queue: Option<QueueDispatch>,
    marker: PhantomData<fn() -> (M, Condition)>,
}

#[derive(Debug, Clone)]
pub struct UpdateReturning<M, S = M> {
    conditions: Vec<Expression>,
    updates: Vec<FieldUpdate>,
    queue: Option<QueueDispatch>,
    marker: PhantomData<fn() -> (M, S)>,
}

pub fn update<M>() -> Update<M>
where
    M: UpdateModel,
{
    Update {
        conditions: Vec::new(),
        updates: Vec::new(),
        relation_writes: Vec::new(),
        queue: None,
        marker: PhantomData,
    }
}

impl<M, Condition> Update<M, Condition>
where
    M: UpdateModel,
{
    pub fn cond<F>(mut self, closure: F) -> Update<M, HasCondition>
    where
        F: FnOnce(M::Where) -> Expression,
    {
        self.conditions.push(closure(M::Where::default()));

        Update {
            conditions: self.conditions,
            updates: self.updates,
            relation_writes: self.relation_writes,
            queue: self.queue,
            marker: PhantomData,
        }
    }

    pub fn update<F>(mut self, closure: F) -> Self
    where
        M: FindAndUpdateModel,
        F: FnOnce(M::Update) -> FieldUpdate,
    {
        self.updates.push(closure(M::Update::default()));

        self
    }

    pub fn connect<F>(mut self, closure: F) -> Self
    where
        M: RelationMutationModel,
        F: FnOnce(M::Relations) -> RelationMutationTarget,
    {
        let target = closure(M::Relations::default());
        let plan = M::relation_write_plan(target).expect("unsupported relation in update().connect()");
        self.relation_writes.push((RelationWriteAction::Connect, plan));

        self
    }

    pub fn disconnect<F>(mut self, closure: F) -> Self
    where
        M: RelationMutationModel,
        F: FnOnce(M::Relations) -> RelationMutationTarget,
    {
        let target = closure(M::Relations::default());
        let plan = M::relation_write_plan(target).expect("unsupported relation in update().disconnect()");
        self.relation_writes.push((RelationWriteAction::Disconnect, plan));

        self
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
}

impl<M> Update<M, HasCondition>
where
    M: UpdateModel,
{
    pub fn returning(self) -> UpdateReturning<M, M>
    where
        M: Projection<M>,
    {
        self.returning_as::<M>()
    }

    pub fn returning_as<S>(self) -> UpdateReturning<M, S>
    where
        M: Projection<M>,
        S: Projection<M>,
    {
        UpdateReturning { conditions: self.conditions, updates: self.updates, queue: self.queue, marker: PhantomData }
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

            if !self.updates.is_empty() {
                execute_update_fields::<M, A>(conditions.clone(), self.updates, client).await?;
            } else if self.relation_writes.is_empty() {
                return Err(dinoco_engine::DinocoError::ParseError(
                    "update() requires at least one update().".to_string(),
                ));
            }

            if !self.relation_writes.is_empty() {
                execute_relation_writes(M::table_name(), conditions.clone(), self.relation_writes, client).await?;
            }

            if let Some(queue) = &self.queue {
                enqueue_single_conditions(client, queue, conditions).await?;
            }

            Ok(())
        }
    }
}

impl<M, S> UpdateReturning<M, S>
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
