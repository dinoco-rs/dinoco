use std::marker::PhantomData;

use dinoco_engine::{DeleteQuery, DinocoClient, DinocoEntity, DinocoProjection, DinocoRowModel, FindWhere};

pub struct DeleteMany<M> {
    conditions: Vec<FindWhere>,
    marker: PhantomData<M>,
}

pub struct DeleteManyReturning<M, S> {
    conditions: Vec<FindWhere>,
    marker: PhantomData<fn() -> (M, S)>,
}

pub fn delete_many<M>() -> DeleteMany<M>
where
    M: DinocoEntity,
{
    DeleteMany { conditions: Vec::new(), marker: PhantomData }
}

impl<M> DeleteMany<M>
where
    M: DinocoEntity,
{
    pub fn where_<F>(mut self, callback: F) -> Self
    where
        F: FnOnce(M::Where) -> FindWhere,
    {
        self.conditions.push(callback(M::Where::default()));

        self
    }

    pub fn returning<S>(self) -> DeleteManyReturning<M, S>
    where
        S: DinocoProjection<M>,
    {
        DeleteManyReturning { conditions: self.conditions, marker: PhantomData }
    }

    pub async fn execute(self, client: &DinocoClient) -> anyhow::Result<()> {
        let query = DeleteQuery { table: M::TABLE_NAME, conditions: self.conditions, returning: None };

        client.backend.delete(query).await?;

        Ok(())
    }
}

impl<M, S> DeleteManyReturning<M, S>
where
    M: DinocoEntity,
    S: DinocoProjection<M> + DinocoRowModel,
{
    pub async fn execute(self, client: &DinocoClient) -> anyhow::Result<Vec<S>> {
        let query = DeleteQuery { table: M::TABLE_NAME, conditions: self.conditions, returning: Some(S::FIELDS) };

        client.backend.delete_returning::<S>(query).await
    }
}
