use std::marker::PhantomData;

use dinoco_engine::{DeleteQuery, DinocoEntity, DinocoProjection, DinocoRowModel, FindWhere, TransactionCommand};

use crate::{DeleteError, IntoTransactionOperation, MutationExecutor};
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

    pub async fn execute<C>(self, client: C) -> anyhow::Result<()>
    where
        C: MutationExecutor,
    {
        let query = DeleteQuery { table: M::TABLE_NAME, conditions: self.conditions, returning: None };

        client.delete(query).await.map_err(DeleteError::from_database)?;

        Ok(())
    }
}

impl<M, S> DeleteManyReturning<M, S>
where
    M: DinocoEntity,
    S: DinocoProjection<M> + DinocoRowModel,
{
    pub async fn execute<C>(self, client: C) -> anyhow::Result<Vec<S>>
    where
        C: MutationExecutor,
    {
        let query = DeleteQuery { table: M::TABLE_NAME, conditions: self.conditions, returning: Some(S::FIELDS) };

        client.delete_returning::<S>(query).await.map_err(|error| DeleteError::from_database(error).into())
    }
}

impl<M> IntoTransactionOperation for DeleteMany<M>
where
    M: DinocoEntity,
{
    fn into_transaction_operation(self) -> TransactionCommand {
        TransactionCommand::delete(DeleteQuery { table: M::TABLE_NAME, conditions: self.conditions, returning: None })
    }
}

impl<M, S> IntoTransactionOperation for DeleteManyReturning<M, S>
where
    M: DinocoEntity,
    S: DinocoProjection<M> + DinocoRowModel,
{
    fn into_transaction_operation(self) -> TransactionCommand {
        TransactionCommand::delete_returning::<S>(DeleteQuery {
            table: M::TABLE_NAME,
            conditions: self.conditions,
            returning: Some(S::FIELDS),
        })
    }
}
