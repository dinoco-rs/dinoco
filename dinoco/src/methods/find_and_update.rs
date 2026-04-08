use std::marker::PhantomData;

use dinoco_engine::{DinocoAdapter, DinocoClient, Expression};

use crate::{FieldUpdate, FindAndUpdateModel, execute_find_and_update};

#[derive(Debug, Clone)]
pub struct FindAndUpdate<M> {
    conditions: Vec<Expression>,
    updates: Vec<FieldUpdate>,
    marker: PhantomData<fn() -> M>,
}

pub fn find_and_update<M>() -> FindAndUpdate<M>
where
    M: FindAndUpdateModel,
{
    FindAndUpdate { conditions: Vec::new(), updates: Vec::new(), marker: PhantomData }
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

    pub fn execute<'a, A>(
        self,
        client: &'a DinocoClient<A>,
    ) -> impl std::future::Future<Output = dinoco_engine::DinocoResult<M>> + 'a
    where
        M: 'a,
        A: DinocoAdapter,
    {
        async move { execute_find_and_update::<M, A>(self.conditions, self.updates, client).await }
    }
}
