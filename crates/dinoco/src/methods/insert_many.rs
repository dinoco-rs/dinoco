use std::future::Future;
use std::marker::PhantomData;

use dinoco_engine::{DinocoEntity, DinocoProjection, DinocoRowModel, PluckValue};
use futures_util::TryFutureExt;

use crate::{
    ConflictMode, ConflictTarget, CreateError, DinocoInsertable, Field, InsertPayload, IntoConflictSets,
    MutationExecutor, NoConflict, OnConflict, OnConflictUpdate, conflict_sets, execute_insert_on_conflict,
    execute_insert_or_existing, execute_insert_owned, execute_insert_owned_returning,
    execute_insert_owned_returning_field, execute_insert_payloads, execute_insert_payloads_returning, reload_inserted,
    reload_inserted_field, single_field,
};

/// `K` is [`OnConflict`] after `.on_conflict_do_nothing(...)` and
/// [`OnConflictUpdate`] after `.on_conflict_do_update(...)`; a plain insert
/// carries the zero-sized [`NoConflict`].
pub struct InsertMany<M, V = M, K = NoConflict> {
    items: Vec<V>,
    conflict: K,
    marker: PhantomData<M>,
}

pub struct InsertManyReturning<M, V = M, S = M, K = NoConflict> {
    items: Vec<V>,
    conflict: K,
    marker: PhantomData<fn() -> (M, S)>,
}

pub struct InsertManyPluck<M, V = M, T = M, K = NoConflict> {
    items: Vec<V>,
    field: &'static str,
    conflict: K,
    marker: PhantomData<fn() -> (M, T)>,
}

pub fn insert_many<M>() -> InsertMany<M>
where
    M: DinocoInsertable,
{
    InsertMany { items: Vec::new(), conflict: NoConflict, marker: PhantomData }
}

impl<M, V> InsertMany<M, V>
where
    M: DinocoInsertable + DinocoProjection<M> + DinocoRowModel + 'static,
    V: InsertPayload<M>,
{
    /// Skips every row that collides with an existing one (or with an earlier
    /// row of the batch) on the unique index over `target` (one field, or a
    /// tuple of fields), instead of failing the whole batch with
    /// [`CreateError::UniqueViolation`]; the other rows are inserted.
    ///
    /// `.returning::<S>()` and `.pluck(...)` then read back one row per
    /// payload, in payload order: the row inserted for it, or the one already
    /// there. MySQL has no conflict target: there a duplicate on any unique
    /// index is skipped.
    pub fn on_conflict_do_nothing<F, Target>(self, callback: F) -> InsertMany<M, V, OnConflict>
    where
        F: FnOnce(M::Where) -> Target,
        Target: ConflictTarget,
    {
        let target = callback(M::Where::default()).conflict_fields(<M as DinocoEntity>::FIELDS);

        InsertMany { items: self.items, conflict: OnConflict::new(target), marker: PhantomData }
    }

    /// Updates the existing row of every payload that collides with one on
    /// the unique index over `target` (one field, or a tuple of fields),
    /// instead of failing the whole batch with
    /// [`CreateError::UniqueViolation`]; the other rows are inserted. Takes
    /// the same assignments as `insert_into(...).on_conflict_do_update(...)`.
    ///
    /// `.returning::<S>()` and `.pluck(...)` then read back one row per
    /// payload, in payload order, as it is after the statement. Two payloads
    /// colliding on the same row fail on PostgreSQL, which updates a row once
    /// per statement; SQLite and MySQL apply them in order. MySQL has no
    /// conflict target: there a duplicate on any unique index updates the row.
    pub fn on_conflict_do_update<F, Target, U, S>(self, target: F, update: U) -> InsertMany<M, V, OnConflictUpdate>
    where
        F: FnOnce(M::Where) -> Target,
        Target: ConflictTarget,
        U: FnOnce(M::Update) -> S,
        S: IntoConflictSets,
    {
        let target = target(M::Where::default()).conflict_fields(<M as DinocoEntity>::FIELDS);
        let sets = conflict_sets(update(M::Update::default()));

        InsertMany { items: self.items, conflict: OnConflictUpdate::new(target, sets), marker: PhantomData }
    }
}

impl<M, V, K> InsertMany<M, V, K>
where
    M: DinocoInsertable + DinocoProjection<M> + DinocoRowModel + 'static,
    V: InsertPayload<M>,
    K: ConflictMode,
{
    pub fn values<N, I>(self, items: I) -> InsertMany<M, N, K>
    where
        N: InsertPayload<M>,
        I: IntoIterator<Item = N>,
    {
        InsertMany { items: items.into_iter().collect(), conflict: self.conflict, marker: PhantomData }
    }

    pub fn returning<S>(self) -> InsertManyReturning<M, V, S, K>
    where
        S: DinocoProjection<M>,
    {
        InsertManyReturning { items: self.items, conflict: self.conflict, marker: PhantomData }
    }

    /// Inserts every row and returns a single column back instead of a full
    /// row model (e.g. `Vec<i64>` for an `id` field).
    pub fn pluck<F, T, C>(self, callback: F) -> InsertManyPluck<M, V, T, K>
    where
        F: FnOnce(M::Where) -> Field<T, C>,
        PluckValue<T>: DinocoRowModel,
    {
        let field = callback(M::Where::default()).field_name();

        InsertManyPluck { items: self.items, field, conflict: self.conflict, marker: PhantomData }
    }

    // Not an `async fn`: the builder is taken apart first, so the future
    // keeps each part once instead of the builder and a copy of it.
    pub fn execute<C>(self, client: C) -> impl Future<Output = Result<(), CreateError>>
    where
        C: MutationExecutor,
    {
        let Self { items, conflict, .. } = self;

        async move {
            if !K::FAILS {
                // Boxed: handling conflicts keeps its state out of every plain insert.
                return Box::pin(execute_insert_on_conflict::<M, V, C>(items, conflict.into_conflict(), &client)).await;
            }

            if V::HAS_NESTED {
                execute_insert_payloads::<M, V, V, C>(&items, &client).await.map_err(CreateError::from_database)?;
            } else {
                execute_insert_owned::<M, V, C>(items, &client).await.map_err(CreateError::from_database)?;
            }

            Ok(())
        }
    }
}

impl<M, V, S, K> InsertManyReturning<M, V, S, K>
where
    M: DinocoInsertable + DinocoProjection<M> + DinocoRowModel + 'static,
    V: InsertPayload<M>,
    S: DinocoProjection<M> + DinocoRowModel,
    K: ConflictMode,
{
    /// Applies a post-query mapping to every returned row.
    ///
    /// This is not a SQL projection: the full row returned by `.returning::<S>()`
    /// is fetched first, then mapped on the Rust side.
    pub fn transform<F, R>(self, callback: F) -> InsertManyReturningTransform<M, V, S, F, K>
    where
        F: FnMut(S) -> R,
    {
        InsertManyReturningTransform { inner: self, transform: callback }
    }

    pub fn execute<C>(self, client: C) -> impl Future<Output = Result<Vec<S>, CreateError>>
    where
        C: MutationExecutor,
    {
        let Self { items, conflict, .. } = self;

        async move {
            if !K::FAILS {
                // Boxed: the read back keeps its state out of every plain insert.
                return Box::pin(execute_insert_or_existing::<M, V, S, C>(
                    items,
                    conflict.into_conflict(),
                    S::FIELDS,
                    &client,
                ))
                .await;
            }

            if !V::HAS_NESTED {
                return execute_insert_owned_returning::<M, V, S, C>(items, &client)
                    .await
                    .map_err(CreateError::from_database);
            }

            let inserted = execute_insert_payloads_returning::<M, V, V, C>(&items, &client)
                .await
                .map_err(CreateError::from_database)?;
            reload_inserted::<M, S, C>(&inserted, &client).await.map_err(CreateError::from_database)
        }
    }
}

impl<M, V, T, K> InsertManyPluck<M, V, T, K>
where
    M: DinocoInsertable + DinocoProjection<M> + DinocoRowModel + 'static,
    V: InsertPayload<M>,
    PluckValue<T>: DinocoRowModel,
    K: ConflictMode,
{
    pub fn execute<C>(self, client: C) -> impl Future<Output = Result<Vec<T>, CreateError>>
    where
        C: MutationExecutor,
    {
        let Self { items, field, conflict, .. } = self;

        async move {
            if !K::FAILS {
                let fields = single_field(field, <M as DinocoEntity>::FIELDS);
                let rows = Box::pin(execute_insert_or_existing::<M, V, PluckValue<T>, C>(
                    items,
                    conflict.into_conflict(),
                    fields,
                    &client,
                ))
                .await?;

                return Ok(rows.into_iter().map(|value| value.0).collect());
            }

            if !V::HAS_NESTED {
                return execute_insert_owned_returning_field::<M, V, T, C>(items, field, &client)
                    .await
                    .map_err(CreateError::from_database);
            }

            let inserted = execute_insert_payloads_returning::<M, V, V, C>(&items, &client)
                .await
                .map_err(CreateError::from_database)?;
            reload_inserted_field::<M, T, C>(&inserted, field, &client).await.map_err(CreateError::from_database)
        }
    }
}

pub struct InsertManyReturningTransform<M, V, S, F, K = NoConflict> {
    inner: InsertManyReturning<M, V, S, K>,
    transform: F,
}

impl<M, V, S, F, R, K> InsertManyReturningTransform<M, V, S, F, K>
where
    M: DinocoInsertable + DinocoProjection<M> + DinocoRowModel + 'static,
    V: InsertPayload<M>,
    S: DinocoProjection<M> + DinocoRowModel,
    F: FnMut(S) -> R,
    K: ConflictMode,
{
    pub fn execute<C>(self, client: C) -> impl Future<Output = Result<Vec<R>, CreateError>>
    where
        C: MutationExecutor,
    {
        let transform = self.transform;

        self.inner.execute(client).map_ok(|rows| rows.into_iter().map(transform).collect())
    }
}
