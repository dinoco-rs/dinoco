use std::marker::PhantomData;

use dinoco_engine::{DinocoProjection, DinocoRowModel, PluckValue};

use crate::{
    CreateError, DinocoInsertable, Field, InsertPayload, MutationExecutor, execute_insert_models_returning,
    execute_insert_models_returning_field, execute_insert_payloads, execute_insert_payloads_returning,
    reload_inserted, reload_inserted_field,
};

pub struct InsertMany<M, V = M> {
    items: Vec<V>,
    marker: PhantomData<M>,
}

pub struct InsertManyReturning<M, V = M, S = M> {
    items: Vec<V>,
    marker: PhantomData<fn() -> (M, S)>,
}

pub struct InsertManyPluck<M, V = M, T = M> {
    items: Vec<V>,
    field: &'static str,
    marker: PhantomData<fn() -> (M, T)>,
}

pub fn insert_many<M>() -> InsertMany<M>
where
    M: DinocoInsertable,
{
    InsertMany { items: Vec::new(), marker: PhantomData }
}

impl<M, V> InsertMany<M, V>
where
    M: DinocoInsertable + DinocoProjection<M> + DinocoRowModel + 'static,
    V: InsertPayload<M>,
{
    pub fn values<N, I>(self, items: I) -> InsertMany<M, N>
    where
        N: InsertPayload<M>,
        I: IntoIterator<Item = N>,
    {
        InsertMany { items: items.into_iter().collect(), marker: PhantomData }
    }

    pub fn returning<S>(self) -> InsertManyReturning<M, V, S>
    where
        S: DinocoProjection<M>,
    {
        InsertManyReturning { items: self.items, marker: PhantomData }
    }

    /// Inserts every row and returns a single column back instead of a full
    /// row model (e.g. `Vec<i64>` for an `id` field).
    pub fn pluck<F, T, C>(self, callback: F) -> InsertManyPluck<M, V, T>
    where
        F: FnOnce(M::Where) -> Field<T, C>,
        PluckValue<T>: DinocoRowModel,
    {
        let field = callback(M::Where::default()).field_name();

        InsertManyPluck { items: self.items, field, marker: PhantomData }
    }

    pub async fn execute<C>(self, client: C) -> anyhow::Result<()>
    where
        C: MutationExecutor,
    {
        execute_insert_payloads::<M, V, V, C>(&self.items, &client).await.map_err(CreateError::from_database)?;

        Ok(())
    }
}

impl<M, V, S> InsertManyReturning<M, V, S>
where
    M: DinocoInsertable + DinocoProjection<M> + DinocoRowModel + 'static,
    V: InsertPayload<M>,
    S: DinocoProjection<M> + DinocoRowModel,
{
    /// Applies a post-query mapping to every returned row.
    ///
    /// This is not a SQL projection: the full row returned by `.returning::<S>()`
    /// is fetched first, then mapped on the Rust side.
    pub fn transform<F, R>(self, callback: F) -> InsertManyReturningTransform<M, V, S, F>
    where
        F: FnMut(S) -> R,
    {
        InsertManyReturningTransform { inner: self, transform: callback }
    }

    pub async fn execute<C>(self, client: C) -> anyhow::Result<Vec<S>>
    where
        C: MutationExecutor,
    {
        if !V::HAS_NESTED {
            let models = self.items.iter().map(InsertPayload::dinoco_insert_model).collect::<Vec<_>>();

            return execute_insert_models_returning::<M, S, C>(&models, &client)
                .await
                .map_err(|error| CreateError::from_database(error).into());
        }

        let inserted = execute_insert_payloads_returning::<M, V, V, C>(&self.items, &client)
            .await
            .map_err(CreateError::from_database)?;
        reload_inserted::<M, S, C>(&inserted, &client).await.map_err(|error| CreateError::from_database(error).into())
    }
}

impl<M, V, T> InsertManyPluck<M, V, T>
where
    M: DinocoInsertable + DinocoProjection<M> + DinocoRowModel + 'static,
    V: InsertPayload<M>,
    PluckValue<T>: DinocoRowModel,
{
    pub async fn execute<C>(self, client: C) -> anyhow::Result<Vec<T>>
    where
        C: MutationExecutor,
    {
        if !V::HAS_NESTED {
            let models = self.items.iter().map(InsertPayload::dinoco_insert_model).collect::<Vec<_>>();

            return execute_insert_models_returning_field::<M, T, C>(&models, self.field, &client)
                .await
                .map_err(|error| CreateError::from_database(error).into());
        }

        let inserted = execute_insert_payloads_returning::<M, V, V, C>(&self.items, &client)
            .await
            .map_err(CreateError::from_database)?;
        reload_inserted_field::<M, T, C>(&inserted, self.field, &client)
            .await
            .map_err(|error| CreateError::from_database(error).into())
    }
}

pub struct InsertManyReturningTransform<M, V, S, F> {
    inner: InsertManyReturning<M, V, S>,
    transform: F,
}

impl<M, V, S, F, R> InsertManyReturningTransform<M, V, S, F>
where
    M: DinocoInsertable + DinocoProjection<M> + DinocoRowModel + 'static,
    V: InsertPayload<M>,
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
