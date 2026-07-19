use std::marker::PhantomData;

use dinoco_engine::{DeleteQuery, DinocoClient, DinocoEntity, DinocoProjection, DinocoRowModel, FindWhere};

pub struct DeleteNeedsWhere;
pub struct DeleteReady;

pub struct Delete<M, State = DeleteNeedsWhere> {
    conditions: Vec<FindWhere>,
    marker: PhantomData<fn() -> (M, State)>,
}

pub struct DeleteReturning<M, S> {
    conditions: Vec<FindWhere>,
    marker: PhantomData<fn() -> (M, S)>,
}

pub fn delete<M>() -> Delete<M>
where
    M: DinocoEntity,
{
    Delete { conditions: Vec::new(), marker: PhantomData }
}

impl<M> Delete<M, DeleteNeedsWhere>
where
    M: DinocoEntity,
{
    pub fn where_<F>(mut self, callback: F) -> Delete<M, DeleteReady>
    where
        F: FnOnce(M::Where) -> FindWhere,
    {
        self.conditions.push(callback(M::Where::default()));

        Delete { conditions: self.conditions, marker: PhantomData }
    }
}

impl<M> Delete<M, DeleteReady>
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

    pub fn returning<S>(self) -> DeleteReturning<M, S>
    where
        S: DinocoProjection<M>,
    {
        DeleteReturning { conditions: self.conditions, marker: PhantomData }
    }

    pub async fn execute(self, client: &DinocoClient) -> anyhow::Result<()> {
        let query = DeleteQuery { table: M::TABLE_NAME, conditions: self.conditions, returning: None };

        client.backend.delete(query).await?;

        Ok(())
    }
}

impl<M, S> DeleteReturning<M, S>
where
    M: DinocoEntity,
    S: DinocoProjection<M> + DinocoRowModel,
{
    pub async fn execute(self, client: &DinocoClient) -> anyhow::Result<Vec<S>> {
        let query = DeleteQuery { table: M::TABLE_NAME, conditions: self.conditions, returning: Some(S::FIELDS) };

        client.backend.delete_returning::<S>(query).await
    }
}
