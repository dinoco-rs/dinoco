use std::marker::PhantomData;

use dinoco_engine::{DinocoEntity, DinocoProjection, DinocoRowModel, FindQuery, FindWhere, PluckValue, UpdateQuery, UpdateSet};

use crate::{DinocoRelationValue, Field, IntoUpdateSets, load_update_matches};
use crate::{MutationExecutor, UpdateError, prepare_scalar_update};

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

    pub fn update<F, R>(mut self, callback: F) -> Self
    where
        F: FnOnce(M::Update) -> R,
        R: IntoUpdateSets,
    {
        self.sets.extend(callback(M::Update::default()).into_update_sets());

        self
    }

    pub fn returning<S>(self) -> UpdateManyReturning<M, S>
    where
        S: DinocoProjection<M>,
    {
        UpdateManyReturning { sets: self.sets, conditions: self.conditions, marker: PhantomData }
    }

    /// Updates the matching rows and returns a single column back instead of
    /// a full row model (e.g. `Vec<i64>` for an `id` field).
    pub fn pluck<F, T, C>(self, callback: F) -> UpdateManyPluck<M, T>
    where
        F: FnOnce(M::Where) -> Field<T, C>,
        PluckValue<T>: DinocoRowModel,
    {
        let field = callback(M::Where::default()).field_name();

        UpdateManyPluck { sets: self.sets, conditions: self.conditions, field, marker: PhantomData }
    }

    pub async fn execute<C>(self, client: C) -> anyhow::Result<()>
    where
        M: DinocoProjection<M> + DinocoRowModel + DinocoRelationValue,
        C: MutationExecutor,
    {
        let sets = prepare_scalar_update::<M, C>(&self.conditions, self.sets, &client).await?;

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
    /// Applies a post-query mapping to every returned row.
    ///
    /// This is not a SQL projection: the full row returned by `.returning::<S>()`
    /// is fetched first, then mapped on the Rust side.
    pub fn transform<F, R>(self, callback: F) -> UpdateManyReturningTransform<M, S, F>
    where
        F: FnMut(S) -> R,
    {
        UpdateManyReturningTransform { inner: self, transform: callback }
    }

    pub async fn execute<C>(self, client: C) -> anyhow::Result<Vec<S>>
    where
        C: MutationExecutor,
    {
        let sets = prepare_scalar_update::<M, C>(&self.conditions, self.sets, &client).await?;

        if sets.is_empty() {
            return load_update_matches::<M, S, C>(&self.conditions, &client)
                .await
                .map_err(|error| UpdateError::from_database(error).into());
        }

        let query = UpdateQuery { table: M::TABLE_NAME, sets, conditions: self.conditions, returning: Some(S::FIELDS) };

        client.update_returning::<S>(query).await.map_err(|error| UpdateError::from_database(error).into())
    }
}

pub struct UpdateManyPluck<M, T> {
    sets: Vec<UpdateSet>,
    conditions: Vec<FindWhere>,
    field: &'static str,
    marker: PhantomData<fn() -> (M, T)>,
}

impl<M, T> UpdateManyPluck<M, T>
where
    M: DinocoEntity + DinocoProjection<M> + DinocoRowModel + DinocoRelationValue,
    PluckValue<T>: DinocoRowModel,
{
    pub async fn execute<C>(self, client: C) -> anyhow::Result<Vec<T>>
    where
        C: MutationExecutor,
    {
        let fields: &'static [&'static str] = Box::leak(vec![self.field].into_boxed_slice());
        let sets = prepare_scalar_update::<M, C>(&self.conditions, self.sets, &client).await?;

        let rows = if sets.is_empty() {
            let mut query = FindQuery::new(fields, M::TABLE_NAME, -1, -1);
            query.conditions = self.conditions;

            client.query::<PluckValue<T>>(query).await.map_err(UpdateError::from_database)?
        } else {
            let query = UpdateQuery { table: M::TABLE_NAME, sets, conditions: self.conditions, returning: Some(fields) };

            client.update_returning::<PluckValue<T>>(query).await.map_err(UpdateError::from_database)?
        };

        Ok(rows.into_iter().map(|value| value.0).collect())
    }
}

pub struct UpdateManyReturningTransform<M, S, F> {
    inner: UpdateManyReturning<M, S>,
    transform: F,
}

impl<M, S, F, R> UpdateManyReturningTransform<M, S, F>
where
    M: DinocoEntity + DinocoProjection<M> + DinocoRowModel + DinocoRelationValue,
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
