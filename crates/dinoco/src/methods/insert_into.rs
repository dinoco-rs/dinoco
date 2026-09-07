use std::marker::PhantomData;

use dinoco_engine::{DinocoProjection, DinocoRowModel, PluckValue};

use crate::{
    CreateError, DinocoInsertable, Field, InsertPayload, MutationExecutor, execute_insert_models_returning,
    execute_insert_models_returning_field, execute_insert_payloads, execute_insert_payloads_returning,
    reload_inserted, reload_inserted_field,
};

pub struct Insert<M, V = M> {
    item: Option<V>,
    marker: PhantomData<M>,
}

pub struct InsertReturning<M, V = M, S = M> {
    item: Option<V>,
    marker: PhantomData<fn() -> (M, S)>,
}

pub struct InsertPluck<M, V = M, T = M> {
    item: Option<V>,
    field: &'static str,
    marker: PhantomData<fn() -> (M, T)>,
}

pub fn insert_into<M>() -> Insert<M>
where
    M: DinocoInsertable,
{
    Insert { item: None, marker: PhantomData }
}

impl<M, V> Insert<M, V>
where
    M: DinocoInsertable + DinocoProjection<M> + DinocoRowModel + 'static,
    V: InsertPayload<M>,
{
    pub fn values<N>(self, item: N) -> Insert<M, N>
    where
        N: InsertPayload<M>,
    {
        Insert { item: Some(item), marker: PhantomData }
    }

    pub fn value<N>(self, item: N) -> Insert<M, N>
    where
        N: InsertPayload<M>,
    {
        self.values(item)
    }

    pub fn returning<S>(self) -> InsertReturning<M, V, S>
    where
        S: DinocoProjection<M>,
    {
        InsertReturning { item: self.item, marker: PhantomData }
    }

    /// Inserts the row and returns a single column back instead of a full
    /// row model (e.g. `i64` for an `id` field).
    pub fn pluck<F, T, C>(self, callback: F) -> InsertPluck<M, V, T>
    where
        F: FnOnce(M::Where) -> Field<T, C>,
        PluckValue<T>: DinocoRowModel,
    {
        let field = callback(M::Where::default()).field_name();

        InsertPluck { item: self.item, field, marker: PhantomData }
    }

    pub async fn execute<C>(self, client: C) -> anyhow::Result<()>
    where
        C: MutationExecutor,
    {
        let item = self.item.expect("insert_into().values(...) must be called before execute()");
        execute_insert_payloads::<M, V, V, C>(&[item], &client).await.map_err(CreateError::from_database)?;

        Ok(())
    }
}

impl<M, V, S> InsertReturning<M, V, S>
where
    M: DinocoInsertable + DinocoProjection<M> + DinocoRowModel + 'static,
    V: InsertPayload<M>,
    S: DinocoProjection<M> + DinocoRowModel,
{
    /// Applies a post-query mapping to the returned row.
    ///
    /// This is not a SQL projection: the full row returned by `.returning::<S>()`
    /// is fetched first, then mapped on the Rust side.
    pub fn transform<F, R>(self, callback: F) -> InsertReturningTransform<M, V, S, F>
    where
        F: FnOnce(S) -> R,
    {
        InsertReturningTransform { inner: self, transform: callback }
    }

    pub async fn execute<C>(self, client: C) -> anyhow::Result<S>
    where
        C: MutationExecutor,
    {
        let item = self.item.expect("insert_into().values(...) must be called before execute()");

        if !V::HAS_NESTED {
            let mut rows = execute_insert_models_returning::<M, S, C>(&[item.dinoco_insert_model()], &client)
                .await
                .map_err(CreateError::from_database)?;

            return rows.pop().ok_or_else(|| {
                anyhow::anyhow!("Record from table '{}' could not be returned after insert.", M::TABLE_NAME)
            });
        }

        let inserted = execute_insert_payloads_returning::<M, V, V, C>(&[item], &client)
            .await
            .map_err(CreateError::from_database)?;
        let mut rows = reload_inserted::<M, S, C>(&inserted, &client).await.map_err(CreateError::from_database)?;

        rows.pop()
            .ok_or_else(|| anyhow::anyhow!("Record from table '{}' could not be loaded after insert.", M::TABLE_NAME))
    }
}

impl<M, V, T> InsertPluck<M, V, T>
where
    M: DinocoInsertable + DinocoProjection<M> + DinocoRowModel + 'static,
    V: InsertPayload<M>,
    PluckValue<T>: DinocoRowModel,
{
    pub async fn execute<C>(self, client: C) -> anyhow::Result<T>
    where
        C: MutationExecutor,
    {
        let item = self.item.expect("insert_into().values(...) must be called before execute()");

        if !V::HAS_NESTED {
            let mut rows =
                execute_insert_models_returning_field::<M, T, C>(&[item.dinoco_insert_model()], self.field, &client)
                    .await
                    .map_err(CreateError::from_database)?;

            return rows.pop().ok_or_else(|| {
                anyhow::anyhow!("Record from table '{}' could not be returned after insert.", M::TABLE_NAME)
            });
        }

        let inserted = execute_insert_payloads_returning::<M, V, V, C>(&[item], &client)
            .await
            .map_err(CreateError::from_database)?;
        let mut rows =
            reload_inserted_field::<M, T, C>(&inserted, self.field, &client).await.map_err(CreateError::from_database)?;

        rows.pop()
            .ok_or_else(|| anyhow::anyhow!("Record from table '{}' could not be loaded after insert.", M::TABLE_NAME))
    }
}

pub struct InsertReturningTransform<M, V, S, F> {
    inner: InsertReturning<M, V, S>,
    transform: F,
}

impl<M, V, S, F, R> InsertReturningTransform<M, V, S, F>
where
    M: DinocoInsertable + DinocoProjection<M> + DinocoRowModel + 'static,
    V: InsertPayload<M>,
    S: DinocoProjection<M> + DinocoRowModel,
    F: FnOnce(S) -> R,
{
    pub async fn execute<C>(self, client: C) -> anyhow::Result<R>
    where
        C: MutationExecutor,
    {
        let row = self.inner.execute(client).await?;

        Ok((self.transform)(row))
    }
}
