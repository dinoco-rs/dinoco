use std::marker::PhantomData;

use dinoco_engine::{DinocoAdapter, DinocoClient, Expression};

use crate::{UpdateModel, execute_update_many};

#[derive(Debug, Clone)]
pub struct UpdateMany<M> {
    conditions: Vec<Expression>,
    items: Vec<M>,
    marker: PhantomData<fn() -> M>,
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
