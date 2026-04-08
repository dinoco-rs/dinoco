use std::marker::PhantomData;

use dinoco_engine::{DinocoAdapter, DinocoClient, Expression};

use crate::{Projection, UpdateModel, execute_update_many, execute_update_many_returning};

#[derive(Debug, Clone)]
pub struct UpdateMany<M> {
    conditions: Vec<Expression>,
    items: Vec<M>,
    marker: PhantomData<fn() -> M>,
}

#[derive(Debug, Clone)]
pub struct UpdateManyReturning<M, S> {
    conditions: Vec<Expression>,
    items: Vec<M>,
    marker: PhantomData<fn() -> (M, S)>,
}

pub fn update_many<M>() -> UpdateMany<M>
where
    M: UpdateModel,
{
    UpdateMany { conditions: Vec::new(), items: Vec::new(), marker: PhantomData }
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

    pub fn values(mut self, items: Vec<M>) -> Self {
        self.items = items;

        self
    }

    pub fn returning<S>(self) -> UpdateManyReturning<M, S>
    where
        S: Projection<M>,
    {
        UpdateManyReturning { conditions: self.conditions, items: self.items, marker: PhantomData }
    }

    pub fn execute<'a, A>(
        self,
        client: &'a DinocoClient<A>,
    ) -> impl std::future::Future<Output = dinoco_engine::DinocoResult<()>> + 'a
    where
        M: 'a,
        A: DinocoAdapter,
    {
        async move { execute_update_many::<M, A>(self.items, self.conditions, client).await }
    }
}

impl<M, S> UpdateManyReturning<M, S>
where
    M: UpdateModel,
    S: Projection<M>,
{
    pub fn execute<'a, A>(
        self,
        client: &'a DinocoClient<A>,
    ) -> impl std::future::Future<Output = dinoco_engine::DinocoResult<Vec<S>>> + 'a
    where
        M: 'a,
        S: 'a,
        A: DinocoAdapter,
    {
        async move { execute_update_many_returning::<M, S, A>(self.items, self.conditions, client).await }
    }
}
