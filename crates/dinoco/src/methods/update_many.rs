use std::marker::PhantomData;

use dinoco_engine::{DinocoClient, DinocoEntity, DinocoProjection, DinocoRowModel, FindWhere, UpdateQuery, UpdateSet};

use crate::{execute_relation_update_sets, split_update_sets};

pub struct UpdateMany<M> {
    sets: Vec<UpdateSet>,
    conditions: Vec<FindWhere>,
    marker: PhantomData<M>,
}

pub struct UpdateManyReturning<M, S> {
    sets: Vec<UpdateSet>,
    conditions: Vec<FindWhere>,
    marker: PhantomData<fn() -> (M, S)>,
}

pub fn update_many<M>() -> UpdateMany<M>
where
    M: DinocoEntity,
{
    UpdateMany { sets: Vec::new(), conditions: Vec::new(), marker: PhantomData }
}

impl<M> UpdateMany<M>
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

    pub fn update<F>(mut self, callback: F) -> Self
    where
        F: FnOnce(M::Update) -> UpdateSet,
    {
        self.sets.push(callback(M::Update::default()));

        self
    }

    pub fn returning<S>(self) -> UpdateManyReturning<M, S>
    where
        S: DinocoProjection<M>,
    {
        UpdateManyReturning { sets: self.sets, conditions: self.conditions, marker: PhantomData }
    }

    pub async fn execute(self, client: &DinocoClient) -> anyhow::Result<()> {
        if self.sets.is_empty() {
            anyhow::bail!("update_many::<{}>() requires at least one .update(...) call.", M::TABLE_NAME);
        }

        let (sets, connects, disconnects) = split_update_sets(self.sets);

        execute_relation_update_sets(M::TABLE_NAME, &self.conditions, connects, disconnects, client).await?;

        if !sets.is_empty() {
            let query = UpdateQuery { table: M::TABLE_NAME, sets, conditions: self.conditions, returning: None };
            client.backend.update(query).await?;
        }

        Ok(())
    }
}

impl<M, S> UpdateManyReturning<M, S>
where
    M: DinocoEntity,
    S: DinocoProjection<M> + DinocoRowModel,
{
    pub async fn execute(self, client: &DinocoClient) -> anyhow::Result<Vec<S>> {
        if self.sets.is_empty() {
            anyhow::bail!("update_many::<{}>() requires at least one .update(...) call.", M::TABLE_NAME);
        }

        let (sets, connects, disconnects) = split_update_sets(self.sets);

        if !connects.is_empty() || !disconnects.is_empty() {
            anyhow::bail!("update_many::<{}>().returning::<T>() does not support connect/disconnect.", M::TABLE_NAME);
        }

        let query = UpdateQuery { table: M::TABLE_NAME, sets, conditions: self.conditions, returning: Some(S::FIELDS) };

        client.backend.update_returning::<S>(query).await
    }
}
