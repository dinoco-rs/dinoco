use std::future::Future;
use std::marker::PhantomData;

use dinoco_engine::{DinocoEntity, DinocoProjection, DinocoRowModel, PluckValue};
use futures_util::TryFutureExt;

use crate::{
    ConflictMode, ConflictTarget, CreateError, DinocoInsertable, Field, InsertPayload, MutationExecutor, NoConflict,
    OnConflict, execute_insert_or_existing, execute_insert_owned, execute_insert_owned_returning,
    execute_insert_owned_returning_field, execute_insert_payloads, execute_insert_payloads_returning,
    execute_insert_skipping_conflicts, reload_inserted, reload_inserted_field, single_field,
};

/// `K` is [`OnConflict`] after `.on_conflict_do_nothing(...)`; a plain insert
/// carries the zero-sized [`NoConflict`].
pub struct Insert<M, V = M, K = NoConflict> {
    item: Option<V>,
    conflict: K,
    marker: PhantomData<M>,
}

pub struct InsertReturning<M, V = M, S = M, K = NoConflict> {
    item: Option<V>,
    conflict: K,
    marker: PhantomData<fn() -> (M, S)>,
}

pub struct InsertPluck<M, V = M, T = M, K = NoConflict> {
    item: Option<V>,
    field: &'static str,
    conflict: K,
    marker: PhantomData<fn() -> (M, T)>,
}

pub fn insert_into<M>() -> Insert<M>
where
    M: DinocoInsertable,
{
    Insert { item: None, conflict: NoConflict, marker: PhantomData }
}

impl<M, V> Insert<M, V>
where
    M: DinocoInsertable + DinocoProjection<M> + DinocoRowModel + 'static,
    V: InsertPayload<M>,
{
    /// Skips the insert when the row collides with an existing one on the
    /// unique index over `target` (one field, or a tuple of fields), instead
    /// of failing with [`CreateError::UniqueViolation`].
    ///
    /// `.returning::<S>()` and `.pluck(...)` then read back the row already
    /// there, so the call returns the row with those values whether it was
    /// inserted or not. MySQL has no conflict target: there a duplicate on any
    /// unique index is skipped.
    pub fn on_conflict_do_nothing<F, Target>(self, callback: F) -> Insert<M, V, OnConflict>
    where
        F: FnOnce(M::Where) -> Target,
        Target: ConflictTarget,
    {
        let target = callback(M::Where::default()).conflict_fields(<M as DinocoEntity>::FIELDS);

        Insert { item: self.item, conflict: OnConflict::new(target), marker: PhantomData }
    }
}

impl<M, V, K> Insert<M, V, K>
where
    M: DinocoInsertable + DinocoProjection<M> + DinocoRowModel + 'static,
    V: InsertPayload<M>,
    K: ConflictMode,
{
    pub fn values<N>(self, item: N) -> Insert<M, N, K>
    where
        N: InsertPayload<M>,
    {
        Insert { item: Some(item), conflict: self.conflict, marker: PhantomData }
    }

    pub fn value<N>(self, item: N) -> Insert<M, N, K>
    where
        N: InsertPayload<M>,
    {
        self.values(item)
    }

    pub fn returning<S>(self) -> InsertReturning<M, V, S, K>
    where
        S: DinocoProjection<M>,
    {
        InsertReturning { item: self.item, conflict: self.conflict, marker: PhantomData }
    }

    /// Inserts the row and returns a single column back instead of a full
    /// row model (e.g. `i64` for an `id` field).
    pub fn pluck<F, T, C>(self, callback: F) -> InsertPluck<M, V, T, K>
    where
        F: FnOnce(M::Where) -> Field<T, C>,
        PluckValue<T>: DinocoRowModel,
    {
        let field = callback(M::Where::default()).field_name();

        InsertPluck { item: self.item, field, conflict: self.conflict, marker: PhantomData }
    }

    // Not an `async fn`: the payload is taken out of the builder first, so
    // the future keeps it once instead of the builder and a copy of it.
    pub fn execute<C>(self, client: C) -> impl Future<Output = Result<(), CreateError>>
    where
        C: MutationExecutor,
    {
        let item = self.item.expect("insert_into().values(...) must be called before execute()");
        let conflict = self.conflict;

        async move {
            if let Some(target) = conflict.target() {
                // Boxed: skipping conflicts keeps its state out of every plain insert.
                return Box::pin(execute_insert_skipping_conflicts::<M, V, C>(vec![item], target, &client)).await;
            }

            if V::HAS_NESTED {
                execute_insert_payloads::<M, V, V, C>(std::slice::from_ref(&item), &client)
                    .await
                    .map_err(CreateError::from_database)?;
            } else {
                execute_insert_owned::<M, V, C>(vec![item], &client).await.map_err(CreateError::from_database)?;
            }

            Ok(())
        }
    }
}

impl<M, V, S, K> InsertReturning<M, V, S, K>
where
    M: DinocoInsertable + DinocoProjection<M> + DinocoRowModel + 'static,
    V: InsertPayload<M>,
    S: DinocoProjection<M> + DinocoRowModel,
    K: ConflictMode,
{
    /// Applies a post-query mapping to the returned row.
    ///
    /// This is not a SQL projection: the full row returned by `.returning::<S>()`
    /// is fetched first, then mapped on the Rust side.
    pub fn transform<F, R>(self, callback: F) -> InsertReturningTransform<M, V, S, F, K>
    where
        F: FnOnce(S) -> R,
    {
        InsertReturningTransform { inner: self, transform: callback }
    }

    pub fn execute<C>(self, client: C) -> impl Future<Output = Result<S, CreateError>>
    where
        C: MutationExecutor,
    {
        let item = self.item.expect("insert_into().values(...) must be called before execute()");
        let conflict = self.conflict;

        async move {
            if let Some(target) = conflict.target() {
                // Boxed: the read back keeps its state out of every plain insert.
                let mut rows =
                    Box::pin(execute_insert_or_existing::<M, V, S, C>(vec![item], target, S::FIELDS, &client)).await?;

                return rows.pop().ok_or_else(|| CreateError::NotReturned { table: M::TABLE_NAME });
            }

            if !V::HAS_NESTED {
                let mut rows = execute_insert_owned_returning::<M, V, S, C>(vec![item], &client)
                    .await
                    .map_err(CreateError::from_database)?;

                return rows.pop().ok_or_else(|| CreateError::NotReturned { table: M::TABLE_NAME });
            }

            let inserted = execute_insert_payloads_returning::<M, V, V, C>(std::slice::from_ref(&item), &client)
                .await
                .map_err(CreateError::from_database)?;
            let mut rows = reload_inserted::<M, S, C>(&inserted, &client).await.map_err(CreateError::from_database)?;

            rows.pop().ok_or_else(|| CreateError::NotReturned { table: M::TABLE_NAME })
        }
    }
}

impl<M, V, T, K> InsertPluck<M, V, T, K>
where
    M: DinocoInsertable + DinocoProjection<M> + DinocoRowModel + 'static,
    V: InsertPayload<M>,
    PluckValue<T>: DinocoRowModel,
    K: ConflictMode,
{
    pub fn execute<C>(self, client: C) -> impl Future<Output = Result<T, CreateError>>
    where
        C: MutationExecutor,
    {
        let item = self.item.expect("insert_into().values(...) must be called before execute()");
        let (field, conflict) = (self.field, self.conflict);

        async move {
            if let Some(target) = conflict.target() {
                let fields = single_field(field, <M as DinocoEntity>::FIELDS);
                let mut rows =
                    Box::pin(execute_insert_or_existing::<M, V, PluckValue<T>, C>(vec![item], target, fields, &client))
                        .await?;

                return rows
                    .pop()
                    .map(|value| value.0)
                    .ok_or_else(|| CreateError::NotReturned { table: M::TABLE_NAME });
            }

            if !V::HAS_NESTED {
                let mut rows = execute_insert_owned_returning_field::<M, V, T, C>(vec![item], field, &client)
                    .await
                    .map_err(CreateError::from_database)?;

                return rows.pop().ok_or_else(|| CreateError::NotReturned { table: M::TABLE_NAME });
            }

            let inserted = execute_insert_payloads_returning::<M, V, V, C>(std::slice::from_ref(&item), &client)
                .await
                .map_err(CreateError::from_database)?;
            let mut rows = reload_inserted_field::<M, T, C>(&inserted, field, &client)
                .await
                .map_err(CreateError::from_database)?;

            rows.pop().ok_or_else(|| CreateError::NotReturned { table: M::TABLE_NAME })
        }
    }
}

pub struct InsertReturningTransform<M, V, S, F, K = NoConflict> {
    inner: InsertReturning<M, V, S, K>,
    transform: F,
}

impl<M, V, S, F, R, K> InsertReturningTransform<M, V, S, F, K>
where
    M: DinocoInsertable + DinocoProjection<M> + DinocoRowModel + 'static,
    V: InsertPayload<M>,
    S: DinocoProjection<M> + DinocoRowModel,
    F: FnOnce(S) -> R,
    K: ConflictMode,
{
    pub fn execute<C>(self, client: C) -> impl Future<Output = Result<R, CreateError>>
    where
        C: MutationExecutor,
    {
        let transform = self.transform;

        self.inner.execute(client).map_ok(transform)
    }
}
