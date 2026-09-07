use std::marker::PhantomData;

use dinoco_engine::{DinocoClient, DinocoEntity, ExistsQuery, FindWhere, WhereComplex};

pub struct Exists<M> {
    conditions: Vec<FindWhere>,
    complex_where: bool,
    read_primary: bool,
    marker: PhantomData<M>,
}

impl<M> Exists<M>
where
    M: DinocoEntity,
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

    pub fn read_in_primary(mut self) -> Self {
        self.read_primary = true;

        self
    }

    pub async fn execute(self, client: &DinocoClient) -> anyhow::Result<bool> {
        client.read_backend(self.read_primary).exists(ExistsQuery { table: M::TABLE_NAME, conditions: self.conditions }).await
    }
}

pub fn exists<M>() -> Exists<M>
where
    M: DinocoEntity,
{
    Exists { conditions: Vec::new(), complex_where: false, read_primary: false, marker: PhantomData }
}
