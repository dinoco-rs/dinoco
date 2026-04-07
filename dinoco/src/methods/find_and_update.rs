use std::marker::PhantomData;

use dinoco_engine::{DinocoAdapter, DinocoClient, Expression};

use crate::{FieldUpdate, FindAndUpdateModel, Projection, execute_find_and_update};

#[derive(Debug, Clone)]
pub struct FindAndUpdate<M, S = M> {
    conditions: Vec<Expression>,
    updates: Vec<FieldUpdate>,
    marker: PhantomData<fn() -> (M, S)>,
}

pub fn find_and_update<M>() -> FindAndUpdate<M>
where
    M: FindAndUpdateModel,
{
    FindAndUpdate { conditions: Vec::new(), updates: Vec::new(), marker: PhantomData }
}

impl<M, S> FindAndUpdate<M, S>
where
    M: FindAndUpdateModel,
    S: Projection<M>,
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

    pub fn returning<NS>(self) -> FindAndUpdate<M, NS>
    where
        NS: Projection<M>,
    {
        FindAndUpdate { conditions: self.conditions, updates: self.updates, marker: PhantomData }
    }

    pub fn execute<'a, A>(
        self,
        client: &'a DinocoClient<A>,
    ) -> impl std::future::Future<Output = dinoco_engine::DinocoResult<S>> + 'a
    where
        M: 'a,
        S: 'a,
        A: DinocoAdapter,
    {
        async move { execute_find_and_update::<M, S, A>(self.conditions, self.updates, client).await }
    }
}
