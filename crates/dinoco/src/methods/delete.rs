use std::marker::PhantomData;

use dinoco_engine::{DeleteQuery, DinocoEntity, DinocoProjection, DinocoRowModel, FindWhere, PluckValue};

use crate::{DeleteError, Field, MutationExecutor};
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

pub struct DeletePluck<M, T> {
    conditions: Vec<FindWhere>,
    field: &'static str,
    marker: PhantomData<fn() -> (M, T)>,
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

    /// Deletes the matching rows and returns a single column back instead of
    /// a full row model (e.g. `Vec<i64>` for an `id` field).
    pub fn pluck<F, T, C>(self, callback: F) -> DeletePluck<M, T>
    where
        F: FnOnce(M::Where) -> Field<T, C>,
        PluckValue<T>: DinocoRowModel,
    {
        let field = callback(M::Where::default()).field_name();

        DeletePluck { conditions: self.conditions, field, marker: PhantomData }
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

impl<M, S> DeleteReturning<M, S>
where
    M: DinocoEntity,
    S: DinocoProjection<M> + DinocoRowModel,
{
    /// Applies a post-query mapping to every returned row.
    ///
    /// This is not a SQL projection: the full row returned by `.returning::<S>()`
    /// is fetched first, then mapped on the Rust side.
    pub fn transform<F, R>(self, callback: F) -> DeleteReturningTransform<M, S, F>
    where
        F: FnMut(S) -> R,
    {
        DeleteReturningTransform { inner: self, transform: callback }
    }

    pub async fn execute<C>(self, client: C) -> anyhow::Result<Vec<S>>
    where
        C: MutationExecutor,
    {
        let query = DeleteQuery { table: M::TABLE_NAME, conditions: self.conditions, returning: Some(S::FIELDS) };

        client.delete_returning::<S>(query).await.map_err(|error| DeleteError::from_database(error).into())
    }
}

impl<M, T> DeletePluck<M, T>
where
    M: DinocoEntity,
    PluckValue<T>: DinocoRowModel,
{
    pub async fn execute<C>(self, client: C) -> anyhow::Result<Vec<T>>
    where
        C: MutationExecutor,
    {
        let fields: &'static [&'static str] = Box::leak(vec![self.field].into_boxed_slice());
        let query = DeleteQuery { table: M::TABLE_NAME, conditions: self.conditions, returning: Some(fields) };

        let rows = client.delete_returning::<PluckValue<T>>(query).await.map_err(DeleteError::from_database)?;

        Ok(rows.into_iter().map(|value| value.0).collect())
    }
}

pub struct DeleteReturningTransform<M, S, F> {
    inner: DeleteReturning<M, S>,
    transform: F,
}

impl<M, S, F, R> DeleteReturningTransform<M, S, F>
where
    M: DinocoEntity,
    S: DinocoProjection<M> + DinocoRowModel,
    F: FnMut(S) -> R,
{
    pub async fn execute<C>(self, client: C) -> anyhow::Result<Vec<R>>
    where
        C: MutationExecutor,
    {
        let rows = self.inner.execute(client).await?;
        let mut transform = self.transform;

        Ok(rows.into_iter().map(|row| transform(row)).collect())
    }
}
