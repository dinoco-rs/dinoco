use std::marker::PhantomData;

use dinoco_engine::{DinocoAdapter, DinocoClient};

use crate::{InsertModel, execute_insert};

#[derive(Debug, Clone)]
pub struct Insert<M> {
    items: Vec<M>,
    marker: PhantomData<fn() -> M>,
}

pub fn insert_into<M>() -> Insert<M>
where
    M: InsertModel,
{
    Insert { items: Vec::new(), marker: PhantomData }
}

impl<M> Insert<M>
where
    M: InsertModel,
{
    pub fn values(mut self, item: M) -> Self {
        self.items.push(item);

        self
    }

    pub fn many(mut self, items: Vec<M>) -> Self {
        self.items.extend(items);

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
        async move { execute_insert::<M, A>(self.items, client).await }
    }
}
