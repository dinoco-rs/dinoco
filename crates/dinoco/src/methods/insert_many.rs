use std::marker::PhantomData;

use dinoco_engine::{DinocoProjection, DinocoRowModel};

use crate::{
    CreateError, DinocoInsertable, InsertPayload, MutationExecutor, execute_insert_models_returning,
    execute_insert_payloads, execute_insert_payloads_returning, reload_inserted,
};

pub struct InsertMany<M, V = M> {
    items: Vec<V>,
    marker: PhantomData<M>,
}

pub struct InsertManyReturning<M, V = M, S = M> {
    items: Vec<V>,
    marker: PhantomData<fn() -> (M, S)>,
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
