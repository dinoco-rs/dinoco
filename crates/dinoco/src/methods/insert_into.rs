use std::marker::PhantomData;

use dinoco_engine::{DinocoClient, DinocoProjection, DinocoRowModel};

use crate::{
    DinocoInsertable, InsertPayload, execute_insert_models_returning, execute_insert_payloads,
    execute_insert_payloads_returning, reload_inserted,
};

pub struct Insert<M, V = M> {
    item: Option<V>,
    marker: PhantomData<M>,
}

pub struct InsertReturning<M, V = M, S = M> {
    item: Option<V>,
    marker: PhantomData<fn() -> (M, S)>,
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

    pub fn returning<S>(self) -> InsertReturning<M, V, S>
    where
        S: DinocoProjection<M>,
    {
        InsertReturning { item: self.item, marker: PhantomData }
    }

    pub async fn execute(self, client: &DinocoClient) -> anyhow::Result<()> {
        let item = self.item.expect("insert_into().values(...) must be called before execute()");
        execute_insert_payloads::<M, V, V>(&[item], client).await?;

        Ok(())
    }
}

impl<M, V, S> InsertReturning<M, V, S>
where
    M: DinocoInsertable + DinocoProjection<M> + DinocoRowModel + 'static,
    V: InsertPayload<M>,
    S: DinocoProjection<M> + DinocoRowModel,
{
    pub async fn execute(self, client: &DinocoClient) -> anyhow::Result<S> {
        let item = self.item.expect("insert_into().values(...) must be called before execute()");

        if !V::HAS_NESTED {
            let mut rows = execute_insert_models_returning::<M, S>(&[item.dinoco_insert_model()], client).await?;

            return rows.pop().ok_or_else(|| {
                anyhow::anyhow!("Record from table '{}' could not be returned after insert.", M::TABLE_NAME)
            });
        }

        let inserted = execute_insert_payloads_returning::<M, V, V>(&[item], client).await?;
        let mut rows = reload_inserted::<M, S>(&inserted, client).await?;

        rows.pop()
            .ok_or_else(|| anyhow::anyhow!("Record from table '{}' could not be loaded after insert.", M::TABLE_NAME))
    }
}
