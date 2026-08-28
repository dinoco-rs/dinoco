use std::marker::PhantomData;

use dinoco_engine::{DinocoEntity, DinocoProjection, DinocoRowModel, FindWhere, UpdateQuery, UpdateSet};

use crate::{DinocoRelationValue, has_many_to_many_update_sets, load_update_matches};
use crate::{MutationExecutor, UpdateError, duplicate_update_field, execute_relation_update_sets, split_update_sets};

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

    pub async fn execute<C>(self, client: C) -> anyhow::Result<()>
    where
        M: DinocoProjection<M> + DinocoRowModel + DinocoRelationValue,
        C: MutationExecutor,
    {
        if self.sets.is_empty() {
            return Err(UpdateError::InvalidOperation(format!(
                "update_many::<{}>() requires at least one .update(...) call.",
                M::TABLE_NAME
            ))
            .into());
        }
        if let Some(field) = duplicate_update_field(&self.sets) {
            return Err(UpdateError::InvalidOperation(format!(
                "field `{field}` is updated more than once in one statement"
            ))
            .into());
        }

        let has_many_to_many = has_many_to_many_update_sets(&self.sets);
        let (sets, connects, disconnects) = split_update_sets(self.sets);
        let parents = if has_many_to_many {
            load_update_matches::<M, M, C>(&self.conditions, &client).await.map_err(UpdateError::from_database)?
        } else {
            Vec::new()
        };

        execute_relation_update_sets(M::TABLE_NAME, &self.conditions, connects, disconnects, &parents, &client)
            .await
            .map_err(UpdateError::from_database)?;

        if !sets.is_empty() {
            let query = UpdateQuery { table: M::TABLE_NAME, sets, conditions: self.conditions, returning: None };
            client.update(query).await.map_err(UpdateError::from_database)?;
        }

        Ok(())
    }
}

impl<M, S> UpdateManyReturning<M, S>
where
    M: DinocoEntity + DinocoProjection<M> + DinocoRowModel + DinocoRelationValue,
    S: DinocoProjection<M> + DinocoRowModel,
{
    pub async fn execute<C>(self, client: C) -> anyhow::Result<Vec<S>>
    where
        C: MutationExecutor,
    {
        if self.sets.is_empty() {
            return Err(UpdateError::InvalidOperation(format!(
                "update_many::<{}>() requires at least one .update(...) call.",
                M::TABLE_NAME
            ))
            .into());
        }
        if let Some(field) = duplicate_update_field(&self.sets) {
            return Err(UpdateError::InvalidOperation(format!(
                "field `{field}` is updated more than once in one statement"
            ))
            .into());
        }

        let has_many_to_many = has_many_to_many_update_sets(&self.sets);
        let (sets, connects, disconnects) = split_update_sets(self.sets);
        let parents = if has_many_to_many {
            load_update_matches::<M, M, C>(&self.conditions, &client).await.map_err(UpdateError::from_database)?
        } else {
            Vec::new()
        };

        execute_relation_update_sets(M::TABLE_NAME, &self.conditions, connects, disconnects, &parents, &client)
            .await
            .map_err(UpdateError::from_database)?;

        if sets.is_empty() {
            return load_update_matches::<M, S, C>(&self.conditions, &client)
                .await
                .map_err(|error| UpdateError::from_database(error).into());
        }

        let query = UpdateQuery { table: M::TABLE_NAME, sets, conditions: self.conditions, returning: Some(S::FIELDS) };

        client.update_returning::<S>(query).await.map_err(|error| UpdateError::from_database(error).into())
    }
}
