use std::marker::PhantomData;

use dinoco_engine::{DinocoClient, DinocoEntity, FindWhere, WhereComplex};

use crate::{CountLoader, IntoCountLoader, execute_count};

pub struct Count<M>
where
    M: DinocoEntity,
{
    conditions: Vec<FindWhere>,
    complex_where: bool,
    counts: Vec<Box<dyn CountLoader<M::Count>>>,
    marker: PhantomData<M>,
}

pub fn count<M>() -> Count<M>
where
    M: DinocoEntity,
{
    Count { conditions: Vec::new(), complex_where: false, counts: Vec::new(), marker: PhantomData }
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
        if !self.complex_where {
            self.conditions.push(callback(M::Where::default()));
        }

        self
    }

    pub fn where_complex<F>(mut self, callback: F) -> Self
    where
        F: FnOnce(M::Where, WhereComplex) -> FindWhere,
    {
        self.conditions = vec![callback(M::Where::default(), WhereComplex)];
        self.complex_where = true;

        self
    }

    pub fn includes<F, I>(mut self, callback: F) -> Self
    where
        F: FnOnce(M::CountInclude) -> I,
        I: IntoCountLoader<M, M::Count>,
    {
        self.counts.push(callback(M::CountInclude::default()).into_count_loader());

        self
    }

    pub async fn execute(self, client: &DinocoClient) -> anyhow::Result<M::Count>
    where
        M::Count: crate::DinocoCountModel<M>,
    {
        execute_count::<M>(self.conditions, self.counts, client).await
    }
}
