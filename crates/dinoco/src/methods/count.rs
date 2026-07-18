use std::marker::PhantomData;

use dinoco_engine::{DinocoClient, DinocoEntity, FindWhere};

use crate::{CountLoader, IntoCountLoader, execute_count};

pub struct Count<M>
where
    M: DinocoEntity,
{
    conditions: Vec<FindWhere>,
    counts: Vec<Box<dyn CountLoader<M::Count>>>,
    marker: PhantomData<M>,
}

pub fn count<M>() -> Count<M>
where
    M: DinocoEntity,
{
    Count { conditions: Vec::new(), counts: Vec::new(), marker: PhantomData }
}

impl<M> Count<M>
where
    M: DinocoEntity,
    M::Count: 'static,
{
    pub fn where_<F>(mut self, callback: F) -> Self
    where
        F: FnOnce(M::Where) -> FindWhere,
    {
        self.conditions.push(callback(M::Where::default()));

        self
    }

    pub fn includes<F, I>(mut self, callback: F) -> Self
    where
        F: FnOnce(M::Count) -> I,
        I: IntoCountLoader<M, M::Count>,
    {
        self.counts.push(callback(M::Count::default()).into_count_loader());

        self
    }

    pub fn count<F, I>(self, callback: F) -> Self
    where
        F: FnOnce(M::Count) -> I,
        I: IntoCountLoader<M, M::Count>,
    {
        self.includes(callback)
    }

    pub async fn execute(self, client: &DinocoClient) -> anyhow::Result<M::Count>
    where
        M::Count: crate::DinocoCountModel<M>,
    {
        execute_count::<M>(self.conditions, self.counts, client).await
    }
}
