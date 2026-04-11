use std::marker::PhantomData;

use chrono::{DateTime, Utc};

use dinoco_engine::{DinocoAdapter, DinocoClient, Expression, UpdateStatement};

use crate::{
    Projection, RelationMutationModel, RelationMutationTarget, RelationWriteAction, RelationWritePlan, UpdateModel,
    execute_relation_writes, execute_update, execute_update_returning,
    queue::{QueueDispatch, dispatch_update_lookup, enqueue_many_conditions, enqueue_single_conditions},
};

#[derive(Debug, Clone)]
pub struct Update<M> {
    conditions: Vec<Expression>,
    relation_writes: Vec<(RelationWriteAction, RelationWritePlan)>,
    item: Option<M>,
    queue: Option<QueueDispatch>,
    marker: PhantomData<fn() -> M>,
}

#[derive(Debug, Clone)]
pub struct UpdateReturning<M, S> {
    conditions: Vec<Expression>,
    relation_writes: Vec<(RelationWriteAction, RelationWritePlan)>,
    item: Option<M>,
    queue: Option<QueueDispatch>,
    marker: PhantomData<fn() -> (M, S)>,
}

pub fn update<M>() -> Update<M>
where
    M: UpdateModel,
{
    Update { conditions: Vec::new(), relation_writes: Vec::new(), item: None, queue: None, marker: PhantomData }
}

impl<M> Update<M>
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

    pub fn values(mut self, item: M) -> Self {
        self.item = Some(item);

        self
    }

    pub fn returning<S>(self) -> UpdateReturning<M, S>
    where
        M: Projection<M>,
        S: Projection<M>,
    {
        UpdateReturning {
            conditions: self.conditions,
            relation_writes: self.relation_writes,
            item: self.item,
            queue: self.queue,
            marker: PhantomData,
        }
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

    pub fn execute<'a, A>(
        self,
        client: &'a DinocoClient<A>,
    ) -> impl std::future::Future<Output = dinoco_engine::DinocoResult<()>> + 'a
    where
        M: 'a,
        A: DinocoAdapter,
    {
        async move {
            let conditions = self.conditions;
            let queue_lookup = self.item.as_ref().map(|item| dispatch_update_lookup(item, &conditions));

            if let Some(item) = self.item {
                item.validate_update()?;
                let mut statement = UpdateStatement::new().table(M::table_name());

                for (column, value) in M::update_columns().iter().copied().zip(item.into_update_row().into_iter()) {
                    statement = statement.set(column, value);
                }

                for condition in conditions.clone() {
                    statement = statement.condition(condition);
                }

                execute_update(statement, client).await?;
            }

            if !self.relation_writes.is_empty() {
                execute_relation_writes(M::table_name(), conditions.clone(), self.relation_writes, client).await?;
            }

            if let (Some(queue), Some(queue_lookup)) = (&self.queue, queue_lookup) {
                enqueue_single_conditions(client, queue, queue_lookup).await?;
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
    ) -> impl std::future::Future<Output = dinoco_engine::DinocoResult<Vec<S>>> + 'a
    where
        M: 'a,
        S: 'a,
        A: DinocoAdapter,
    {
        async move {
            if !self.relation_writes.is_empty() {
                return Err(dinoco_engine::DinocoError::ParseError(
                    "update().returning() does not support relation writes.".to_string(),
                ));
            }

            let item = self.item.ok_or_else(|| {
                dinoco_engine::DinocoError::ParseError(
                    "update().returning() requires values(...) before execute().".to_string(),
                )
            })?;
            let queue_lookup = dispatch_update_lookup(&item, &self.conditions);
            let result = execute_update_returning::<M, S, A>(self.conditions, item, client).await?;

            if let Some(queue) = &self.queue {
                enqueue_many_conditions(client, queue, vec![queue_lookup]).await?;
            }

            Ok(result)
        }
    }
}
