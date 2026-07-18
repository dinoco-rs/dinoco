use std::marker::PhantomData;

use dinoco_engine::{DinocoClient, DinocoEntity, DinocoProjection, DinocoRowModel, FindWhere, UpdateQuery, UpdateSet};

use crate::split_update_sets;

pub struct FindAndUpdate<M> {
    sets: Vec<UpdateSet>,
    conditions: Vec<FindWhere>,
    marker: PhantomData<M>,
}

pub fn find_and_update<M>() -> FindAndUpdate<M>
where
    M: DinocoEntity,
{
    FindAndUpdate { sets: Vec::new(), conditions: Vec::new(), marker: PhantomData }
}

impl<M> FindAndUpdate<M>
where
    M: DinocoEntity + DinocoProjection<M> + DinocoRowModel,
{
    pub fn where_<F>(mut self, callback: F) -> Self
    where
        F: FnOnce(M::Where) -> FindWhere,
    {
        self.conditions.push(callback(M::Where::default()));

        self
    }

    pub fn update<F>(mut self, callback: F) -> Self
    where
        F: FnOnce(M::Update) -> UpdateSet,
    {
        self.sets.push(callback(M::Update::default()));

        self
    }

    pub async fn execute(self, client: &DinocoClient) -> anyhow::Result<M> {
        if self.sets.is_empty() {
            anyhow::bail!("find_and_update::<{}>() requires at least one .update(...) call.", M::TABLE_NAME);
        }

        let (sets, connects, disconnects) = split_update_sets(self.sets);

        if !connects.is_empty() || !disconnects.is_empty() {
            anyhow::bail!("find_and_update::<{}>() does not support connect/disconnect.", M::TABLE_NAME);
        }

        let query = UpdateQuery {
            table: M::TABLE_NAME,
            sets,
            conditions: self.conditions,
            returning: Some(<M as DinocoProjection<M>>::FIELDS),
        };
        let mut rows = client.backend.update_returning::<M>(query).await?;

        rows.pop()
            .ok_or_else(|| anyhow::anyhow!("Record from table '{}' could not be found for update.", M::TABLE_NAME))
    }
}
