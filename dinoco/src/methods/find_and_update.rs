use std::marker::PhantomData;

use chrono::{DateTime, Utc};

use dinoco_engine::{DinocoAdapter, DinocoClient, Expression};

use crate::{
    FieldUpdate, FindAndUpdateModel, execute_find_and_update,
    queue::{QueueDispatch, enqueue_single_conditions},
};

#[derive(Debug, Clone)]
pub struct FindAndUpdate<M> {
    conditions: Vec<Expression>,
    updates: Vec<FieldUpdate>,
    queue: Option<QueueDispatch>,
    marker: PhantomData<fn() -> M>,
}

pub fn find_and_update<M>() -> FindAndUpdate<M>
where
    M: FindAndUpdateModel,
{
    FindAndUpdate { conditions: Vec::new(), updates: Vec::new(), queue: None, marker: PhantomData }
}

impl<M> FindAndUpdate<M>
where
    M: FindAndUpdateModel,
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
        F: FnOnce(M::Update) -> FieldUpdate,
    {
        self.updates.push(closure(M::Update::default()));
        self
    }

    pub fn enqueue(mut self, event: impl Into<String>) -> Self {
        self.queue = Some(QueueDispatch::immediate(event));

        self
    }

    pub fn enqueue_in(mut self, event: impl Into<String>, delay_ms: u64) -> Self {
        self.queue = Some(QueueDispatch::in_milliseconds(event, delay_ms));

        self
    }

    pub fn enqueue_at(mut self, event: impl Into<String>, execute_at: DateTime<Utc>) -> Self {
        self.queue = Some(QueueDispatch::at(event, execute_at));

        self
    }

    pub fn execute<'a, A>(
        self,
        client: &'a DinocoClient<A>,
    ) -> impl std::future::Future<Output = dinoco_engine::DinocoResult<M>> + 'a
    where
        M: 'a,
        A: DinocoAdapter,
    {
        async move {
            let conditions = self.conditions;
            let result = execute_find_and_update::<M, A>(conditions.clone(), self.updates, client).await?;

            if let Some(queue) = &self.queue {
                enqueue_single_conditions(client, queue, result.update_identity_conditions()).await?;
            }

            Ok(result)
        }
    }
}
