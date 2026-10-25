use std::future::{Future, ready};
use std::marker::PhantomData;

use dinoco_engine::{
    DinocoEntity, DinocoProjection, DinocoRowModel, FindQuery, FindWhere, PluckValue, UpdateQuery, UpdateSet,
};
use futures_util::TryFutureExt;
use futures_util::future::Either;

use crate::{
    DinocoRelationValue, Field, IntoUpdateSets, has_many_to_many_update_sets, load_update_matches, single_field,
};
use crate::{
    MutationExecutor, UpdateError, duplicate_update_field, execute_relation_update_sets, split_update_sets,
    touch_updated_at,
};

pub struct Update<M> {
    sets: Vec<UpdateSet>,
    conditions: Vec<FindWhere>,
    marker: PhantomData<M>,
}

pub struct UpdateReturning<M, S> {
    sets: Vec<UpdateSet>,
    conditions: Vec<FindWhere>,
    marker: PhantomData<fn() -> (M, S)>,
}

pub fn update<M>() -> Update<M>
where
    M: DinocoEntity,
{
    Update { sets: Vec::new(), conditions: Vec::new(), marker: PhantomData }
}

/// Runs the shared preamble for every `update`/`update_many` execution path:
/// validates the `.update(...)` calls, applies relation connects/disconnects,
/// and returns the remaining scalar `UpdateSet`s for the caller to act on
/// (issue a plain `UPDATE`, an `UPDATE ... RETURNING`, or reload the rows).
///
/// Validation and the split run as this is called; the future only awaits
/// relation updates, boxed since they are rare, so every other update keeps
/// just the scalar sets in its state.
pub(crate) fn prepare_scalar_update<'a, M, C>(
    conditions: &'a [FindWhere],
    sets: Vec<UpdateSet>,
    client: &'a C,
) -> impl Future<Output = anyhow::Result<Vec<UpdateSet>>> + 'a
where
    M: DinocoEntity + DinocoProjection<M> + DinocoRowModel + DinocoRelationValue,
    C: MutationExecutor,
{
    if sets.is_empty() {
        return Either::Left(ready(Err(UpdateError::InvalidOperation(format!(
            "update::<{}>() requires at least one .update(...) call.",
            M::TABLE_NAME
        ))
        .into())));
    }
    if let Some(field) = duplicate_update_field(&sets) {
        return Either::Left(ready(Err(UpdateError::InvalidOperation(format!(
            "field `{field}` is updated more than once in one statement"
        ))
        .into())));
    }

    let has_many_to_many = has_many_to_many_update_sets(&sets);
    let (scalar_sets, connects, disconnects) = split_update_sets(sets);
    let scalar_sets = touch_updated_at::<M>(scalar_sets);

    if connects.is_empty() && disconnects.is_empty() {
        return Either::Left(ready(Ok(scalar_sets)));
    }

    // Relation updates are rare; boxed, they keep their state out of every
    // update's future.
    Either::Right(Box::pin(async move {
        let parents = if has_many_to_many {
            load_update_matches::<M, M, C>(conditions, client).await.map_err(UpdateError::from_database)?
        } else {
            Vec::new()
        };

        execute_relation_update_sets(M::TABLE_NAME, conditions, connects, disconnects, &parents, client)
            .await
            .map_err(UpdateError::from_database)?;

        Ok(scalar_sets)
    }))
}

impl<M> Update<M>
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

    pub fn returning<S>(self) -> UpdateReturning<M, S>
    where
        S: DinocoProjection<M>,
    {
        UpdateReturning { sets: self.sets, conditions: self.conditions, marker: PhantomData }
    }

    /// Updates the matching rows and returns a single column back instead of
    /// a full row model (e.g. `Vec<i64>` for an `id` field).
    pub fn pluck<F, T, C>(self, callback: F) -> UpdatePluck<M, T>
    where
        F: FnOnce(M::Where) -> Field<T, C>,
        PluckValue<T>: DinocoRowModel,
    {
        let field = callback(M::Where::default()).field_name();

        UpdatePluck { sets: self.sets, conditions: self.conditions, field, marker: PhantomData }
    }

    // Not an `async fn`: the builder is taken apart first, so the future
    // keeps each part once instead of the builder and a copy of it.
    pub fn execute<C>(self, client: C) -> impl Future<Output = anyhow::Result<()>>
    where
        M: DinocoProjection<M> + DinocoRowModel + DinocoRelationValue,
        C: MutationExecutor,
    {
        let Self { sets, conditions, .. } = self;

        async move {
            let sets = prepare_scalar_update::<M, C>(&conditions, sets, &client).await?;

            if !sets.is_empty() {
                let query = UpdateQuery { table: M::TABLE_NAME, sets, conditions, returning: None };
                client.update(query).await.map_err(UpdateError::from_database)?;
            }

            Ok(())
        }
    }
}

impl<M, S> UpdateReturning<M, S>
where
    M: DinocoEntity + DinocoProjection<M> + DinocoRowModel + DinocoRelationValue,
    S: DinocoProjection<M> + DinocoRowModel,
{
    /// Applies a post-query mapping to every returned row.
    ///
    /// This is not a SQL projection: the full row returned by `.returning::<S>()`
    /// is fetched first, then mapped on the Rust side.
    pub fn transform<F, R>(self, callback: F) -> UpdateReturningTransform<M, S, F>
    where
        F: FnMut(S) -> R,
    {
        UpdateReturningTransform { inner: self, transform: callback }
    }

    pub fn execute<C>(self, client: C) -> impl Future<Output = anyhow::Result<Vec<S>>>
    where
        C: MutationExecutor,
    {
        let Self { sets, conditions, .. } = self;

        async move {
            let sets = prepare_scalar_update::<M, C>(&conditions, sets, &client).await?;

            if sets.is_empty() {
                return load_update_matches::<M, S, C>(&conditions, &client)
                    .await
                    .map_err(|error| UpdateError::from_database(error).into());
            }

            let query = UpdateQuery { table: M::TABLE_NAME, sets, conditions, returning: Some(S::FIELDS) };

            client.update_returning::<S>(query).await.map_err(|error| UpdateError::from_database(error).into())
        }
    }
}

pub struct UpdatePluck<M, T> {
    sets: Vec<UpdateSet>,
    conditions: Vec<FindWhere>,
    field: &'static str,
    marker: PhantomData<fn() -> (M, T)>,
}

impl<M, T> UpdatePluck<M, T>
where
    M: DinocoEntity + DinocoProjection<M> + DinocoRowModel + DinocoRelationValue,
    PluckValue<T>: DinocoRowModel,
{
    pub fn execute<C>(self, client: C) -> impl Future<Output = anyhow::Result<Vec<T>>>
    where
        C: MutationExecutor,
    {
        let fields = single_field(self.field, <M as DinocoEntity>::FIELDS);
        let Self { sets, conditions, .. } = self;

        async move {
            let sets = prepare_scalar_update::<M, C>(&conditions, sets, &client).await?;

            let rows = if sets.is_empty() {
                let mut query = FindQuery::new(fields, M::TABLE_NAME, -1, -1);
                query.conditions = conditions;

                client.query::<PluckValue<T>>(query).await.map_err(UpdateError::from_database)?
            } else {
                let query = UpdateQuery { table: M::TABLE_NAME, sets, conditions, returning: Some(fields) };

                client.update_returning::<PluckValue<T>>(query).await.map_err(UpdateError::from_database)?
            };

            Ok(rows.into_iter().map(|value| value.0).collect())
        }
    }
}

pub struct UpdateReturningTransform<M, S, F> {
    inner: UpdateReturning<M, S>,
    transform: F,
}

impl<M, S, F, R> UpdateReturningTransform<M, S, F>
where
    M: DinocoEntity + DinocoProjection<M> + DinocoRowModel + DinocoRelationValue,
    S: DinocoProjection<M> + DinocoRowModel,
    F: FnMut(S) -> R,
{
    pub fn execute<C>(self, client: C) -> impl Future<Output = anyhow::Result<Vec<R>>>
    where
        C: MutationExecutor,
    {
        let transform = self.transform;

        self.inner.execute(client).map_ok(|rows| rows.into_iter().map(transform).collect())
    }
}
