use std::borrow::Borrow;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicU16, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use dinoco_engine::{
    ConflictSet, DinocoEntity, DinocoProjection, DinocoRowModel, DinocoValue, FindQuery, FindWhere, InsertQuery,
    LookupQuery, PluckValue, UpdateSet, UpsertQuery,
};

use crate::{CreateError, Field, field_list, single_field};

pub type Uuid = String;
pub type Snowflake = i64;

pub type InsertNestedFuture<'a> = Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send + 'a>>;

pub trait DinocoInsertable: DinocoEntity + Sized {
    const INSERT_FIELDS: &'static [&'static str];

    fn dinoco_insert_values(&self) -> Vec<DinocoValue>;
    fn dinoco_insert_identity(&self) -> Vec<FindWhere>;

    /// [`dinoco_insert_values`](Self::dinoco_insert_values), moving the
    /// values out of the model instead of copying them.
    fn dinoco_into_insert_values(self) -> Vec<DinocoValue> {
        self.dinoco_insert_values()
    }
}

pub trait InsertPayload<M>: Sync
where
    M: DinocoInsertable,
{
    const HAS_NESTED: bool = false;

    fn dinoco_insert_model(&self) -> M;

    /// [`dinoco_insert_model`](Self::dinoco_insert_model), consuming the
    /// payload so an owned model can be inserted without being copied.
    fn dinoco_into_insert_model(self) -> M
    where
        Self: Sized,
    {
        self.dinoco_insert_model()
    }

    fn dinoco_insert_nested<'a, C>(&'a self, _parent: &'a M, _client: &'a C) -> InsertNestedFuture<'a>
    where
        C: crate::MutationExecutor + 'a,
    {
        Box::pin(async { Ok(()) })
    }

    /// Whether this payload carries nested rows or many-to-many links for
    /// [`dinoco_insert_nested`](Self::dinoco_insert_nested) to insert.
    fn dinoco_has_nested_values(&self) -> bool {
        Self::HAS_NESTED
    }
}

impl<M, V> InsertPayload<M> for &V
where
    M: DinocoInsertable,
    V: InsertPayload<M>,
{
    const HAS_NESTED: bool = V::HAS_NESTED;

    fn dinoco_insert_model(&self) -> M {
        (*self).dinoco_insert_model()
    }

    fn dinoco_insert_nested<'a, C>(&'a self, parent: &'a M, client: &'a C) -> InsertNestedFuture<'a>
    where
        C: crate::MutationExecutor + 'a,
    {
        (*self).dinoco_insert_nested(parent, client)
    }

    fn dinoco_has_nested_values(&self) -> bool {
        (*self).dinoco_has_nested_values()
    }
}

/// The model a row type reads: itself for a model, the extended model for an
/// `#[derive(EntityExtend)]` projection. A nested insert of a related
/// projection inserts it as that model.
#[doc(hidden)]
pub trait DinocoRowOf {
    type Model: DinocoInsertable;
}

pub trait DinocoBelongsTo<P> {
    fn dinoco_bind_parent(&mut self, parent: &P);

    fn dinoco_bind_parent_relation(&mut self, _relation: &'static str, parent: &P) {
        self.dinoco_bind_parent(parent);
    }
}

pub fn new_uuid() -> Uuid {
    uuid::Uuid::now_v7().to_string()
}

pub fn new_snowflake_id() -> Snowflake {
    static SEQUENCE: AtomicU16 = AtomicU16::new(0);

    let timestamp =
        SystemTime::now().duration_since(UNIX_EPOCH).map(|duration| duration.as_millis() as i64).unwrap_or_default();
    let sequence = (SEQUENCE.fetch_add(1, Ordering::Relaxed) & 0x0fff) as i64;

    (timestamp << 12) | sequence
}

pub async fn execute_insert_payloads<M, V, B, C>(values: &[B], client: &C) -> anyhow::Result<Vec<M>>
where
    M: DinocoInsertable + DinocoProjection<M> + DinocoRowModel,
    V: InsertPayload<M>,
    B: Borrow<V>,
    C: crate::MutationExecutor,
{
    let mut models = values.iter().map(|value| value.borrow().dinoco_insert_model()).collect::<Vec<_>>();

    if !V::HAS_NESTED {
        execute_insert_models::<M, C>(&models, client).await?;
        return Ok(models);
    }

    let inserted = execute_insert_models_returning::<M, M, C>(&models, client).await?;

    for (payload, model) in values.iter().zip(inserted.iter()) {
        payload.borrow().dinoco_insert_nested(model, client).await?;
    }

    models.clear();

    Ok(inserted)
}

pub(crate) async fn execute_insert_payloads_returning<M, V, B, C>(values: &[B], client: &C) -> anyhow::Result<Vec<M>>
where
    M: DinocoInsertable + DinocoProjection<M> + DinocoRowModel,
    V: InsertPayload<M>,
    B: Borrow<V>,
    C: crate::MutationExecutor,
{
    let models = values.iter().map(|value| value.borrow().dinoco_insert_model()).collect::<Vec<_>>();
    let inserted = execute_insert_models_returning::<M, M, C>(&models, client).await?;

    if V::HAS_NESTED {
        for (payload, model) in values.iter().zip(inserted.iter()) {
            payload.borrow().dinoco_insert_nested(model, client).await?;
        }
    }

    Ok(inserted)
}

pub async fn execute_insert_models<M, C>(models: &[M], client: &C) -> anyhow::Result<usize>
where
    M: DinocoInsertable,
    C: crate::MutationExecutor,
{
    if models.is_empty() {
        return Ok(0);
    }

    let rows = models.iter().map(DinocoInsertable::dinoco_insert_values).collect::<Vec<_>>();
    let query = InsertQuery {
        table: M::TABLE_NAME,
        fields: M::INSERT_FIELDS.to_vec(),
        rows,
        returning: None,
        on_conflict_do_nothing: None,
    };

    client.insert(query).await
}

pub async fn execute_insert_models_returning<M, S, C>(models: &[M], client: &C) -> anyhow::Result<Vec<S>>
where
    M: DinocoInsertable,
    S: DinocoProjection<M> + DinocoRowModel,
    C: crate::MutationExecutor,
{
    if models.is_empty() {
        return Ok(Vec::new());
    }

    let rows = models.iter().map(DinocoInsertable::dinoco_insert_values).collect::<Vec<_>>();
    let query = InsertQuery {
        table: M::TABLE_NAME,
        fields: M::INSERT_FIELDS.to_vec(),
        rows,
        returning: Some(S::FIELDS),
        on_conflict_do_nothing: None,
    };

    client.insert_returning::<S>(query).await
}

/// Inserts payloads without nested relations, moving each one into its row.
pub(crate) async fn execute_insert_owned<M, V, C>(values: Vec<V>, client: &C) -> anyhow::Result<usize>
where
    M: DinocoInsertable,
    V: InsertPayload<M>,
    C: crate::MutationExecutor,
{
    if values.is_empty() {
        return Ok(0);
    }

    client.insert(owned_insert_query::<M, V>(values, None, None)).await
}

/// [`execute_insert_owned`] returning the inserted rows projected onto `S`.
pub(crate) async fn execute_insert_owned_returning<M, V, S, C>(values: Vec<V>, client: &C) -> anyhow::Result<Vec<S>>
where
    M: DinocoInsertable,
    V: InsertPayload<M>,
    S: DinocoProjection<M> + DinocoRowModel,
    C: crate::MutationExecutor,
{
    if values.is_empty() {
        return Ok(Vec::new());
    }

    client.insert_returning::<S>(owned_insert_query::<M, V>(values, Some(S::FIELDS), None)).await
}

/// [`execute_insert_owned`] returning one column of the inserted rows.
pub(crate) async fn execute_insert_owned_returning_field<M, V, T, C>(
    values: Vec<V>,
    field: &'static str,
    client: &C,
) -> anyhow::Result<Vec<T>>
where
    M: DinocoInsertable,
    V: InsertPayload<M>,
    PluckValue<T>: DinocoRowModel,
    C: crate::MutationExecutor,
{
    if values.is_empty() {
        return Ok(Vec::new());
    }

    let fields = single_field(field, <M as DinocoEntity>::FIELDS);
    let rows = client.insert_returning::<PluckValue<T>>(owned_insert_query::<M, V>(values, Some(fields), None)).await?;

    Ok(rows.into_iter().map(|value| value.0).collect())
}

fn owned_insert_query<M, V>(
    values: Vec<V>,
    returning: Option<&'static [&'static str]>,
    on_conflict_do_nothing: Option<&'static [&'static str]>,
) -> InsertQuery
where
    M: DinocoInsertable,
    V: InsertPayload<M>,
{
    let rows = values.into_iter().map(|value| value.dinoco_into_insert_model().dinoco_into_insert_values()).collect();

    InsertQuery { table: M::TABLE_NAME, fields: M::INSERT_FIELDS.to_vec(), rows, returning, on_conflict_do_nothing }
}

/// What an insert builder does with a row that collides with an existing one:
/// [`NoConflict`] (zero sized, every plain insert) fails, [`OnConflict`], set
/// by `.on_conflict_do_nothing(...)`, skips it, and [`OnConflictUpdate`], set
/// by `.on_conflict_do_update(...)`, updates the existing row.
pub trait ConflictMode: Send + Sync {
    /// Whether a conflict fails the insert, so the builder runs a plain insert.
    #[doc(hidden)]
    const FAILS: bool = false;

    #[doc(hidden)]
    fn into_conflict(self) -> Conflict;
}

/// How an insert handles conflicts, taken from its [`ConflictMode`].
#[doc(hidden)]
#[derive(Debug)]
pub enum Conflict {
    Fail,
    DoNothing(&'static [&'static str]),
    DoUpdate(&'static [&'static str], Vec<ConflictSet>),
}

/// The conflict mode of a plain insert: a conflict fails it.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoConflict;

/// The conflict mode after `.on_conflict_do_nothing(...)`: a row that collides
/// on these columns is skipped.
#[derive(Debug, Clone, Copy)]
pub struct OnConflict {
    target: &'static [&'static str],
}

impl OnConflict {
    pub(crate) const fn new(target: &'static [&'static str]) -> Self {
        Self { target }
    }
}

/// The conflict mode after `.on_conflict_do_update(...)`: a row that collides
/// on these columns updates the existing row with these assignments.
#[derive(Debug)]
pub struct OnConflictUpdate {
    target: &'static [&'static str],
    sets: Vec<ConflictSet>,
}

impl OnConflictUpdate {
    pub(crate) fn new(target: &'static [&'static str], sets: Vec<ConflictSet>) -> Self {
        Self { target, sets }
    }
}

impl ConflictMode for NoConflict {
    const FAILS: bool = true;

    #[inline]
    fn into_conflict(self) -> Conflict {
        Conflict::Fail
    }
}

impl ConflictMode for OnConflict {
    #[inline]
    fn into_conflict(self) -> Conflict {
        Conflict::DoNothing(self.target)
    }
}

impl ConflictMode for OnConflictUpdate {
    #[inline]
    fn into_conflict(self) -> Conflict {
        Conflict::DoUpdate(self.target, self.sets)
    }
}

/// The assignments `.on_conflict_do_update(...)` applies to the existing row:
/// an [`UpdateSet`] (`.set(...)`, `.increment(...)`, ...) or a
/// [`ConflictSet`] (`.excluded()`, the value the insert proposed), a `Vec` or
/// array of either, or a tuple mixing them.
///
/// ```ignore
/// .on_conflict_do_update(|x| x.path, |x| (x.title.excluded(), x.hits.increment(1)))
/// ```
pub trait IntoConflictSets {
    #[doc(hidden)]
    fn push_conflict_sets(self, sets: &mut Vec<ConflictSet>);
}

impl IntoConflictSets for UpdateSet {
    fn push_conflict_sets(self, sets: &mut Vec<ConflictSet>) {
        sets.push(ConflictSet::Update(self));
    }
}

impl IntoConflictSets for ConflictSet {
    fn push_conflict_sets(self, sets: &mut Vec<ConflictSet>) {
        sets.push(self);
    }
}

impl<T> IntoConflictSets for Vec<T>
where
    T: IntoConflictSets,
{
    fn push_conflict_sets(self, sets: &mut Vec<ConflictSet>) {
        for set in self {
            set.push_conflict_sets(sets);
        }
    }
}

impl<T, const N: usize> IntoConflictSets for [T; N]
where
    T: IntoConflictSets,
{
    fn push_conflict_sets(self, sets: &mut Vec<ConflictSet>) {
        for set in self {
            set.push_conflict_sets(sets);
        }
    }
}

macro_rules! conflict_set_tuples {
    ($(($($set:ident: $ty:ident),+)),* $(,)?) => {
        $(
            impl<$($ty),+> IntoConflictSets for ($($ty,)+)
            where
                $($ty: IntoConflictSets,)+
            {
                fn push_conflict_sets(self, sets: &mut Vec<ConflictSet>) {
                    let ($($set,)+) = self;
                    $($set.push_conflict_sets(sets);)+
                }
            }
        )*
    };
}

conflict_set_tuples!(
    (a: A),
    (a: A, b: B),
    (a: A, b: B, c: C),
    (a: A, b: B, c: C, d: D),
    (a: A, b: B, c: C, d: D, e: E),
    (a: A, b: B, c: C, d: D, e: E, f: F),
    (a: A, b: B, c: C, d: D, e: E, f: F, g: G),
    (a: A, b: B, c: C, d: D, e: E, f: F, g: G, h: H),
);

/// The assignments of `.on_conflict_do_update(...)`, collected from
/// `callback`.
pub(crate) fn conflict_sets<S>(update: S) -> Vec<ConflictSet>
where
    S: IntoConflictSets,
{
    let mut sets = Vec::new();
    update.push_conflict_sets(&mut sets);
    sets
}

/// The columns `.on_conflict_do_nothing(...)` names: one [`Field`], or a
/// tuple of up to four for a unique index over several columns.
pub trait ConflictTarget {
    #[doc(hidden)]
    fn conflict_fields(self, fields: &'static [&'static str]) -> &'static [&'static str];
}

impl<T, C> ConflictTarget for Field<T, C> {
    fn conflict_fields(self, fields: &'static [&'static str]) -> &'static [&'static str] {
        single_field(self.field_name(), fields)
    }
}

macro_rules! conflict_target_tuples {
    ($(($($field:ident: $ty:ident, $capability:ident),+)),* $(,)?) => {
        $(
            impl<$($ty, $capability),+> ConflictTarget for ($(Field<$ty, $capability>,)+) {
                fn conflict_fields(self, fields: &'static [&'static str]) -> &'static [&'static str] {
                    let ($($field,)+) = self;
                    field_list(&[$($field.field_name()),+], fields)
                }
            }
        )*
    };
}

conflict_target_tuples!(
    (a: A, CA, b: B, CB),
    (a: A, CA, b: B, CB, c: C, CC),
    (a: A, CA, b: B, CB, c: C, CC, d: D, CD),
);

impl Conflict {
    /// The builder method that set this mode, for error messages.
    fn method(&self) -> &'static str {
        match self {
            Conflict::DoUpdate(..) => "on_conflict_do_update(...)",
            Conflict::Fail | Conflict::DoNothing(_) => "on_conflict_do_nothing(...)",
        }
    }

    fn target(&self) -> &'static [&'static str] {
        match self {
            Conflict::Fail => &[],
            Conflict::DoNothing(target) | Conflict::DoUpdate(target, _) => target,
        }
    }
}

/// `on_conflict_do_nothing(...)` and `on_conflict_do_update(...)` insert
/// top-level rows only: a skipped payload has no inserted row for its nested
/// relations to hang from.
pub(crate) fn reject_nested_on_conflict<M, V>(values: &[V], method: &str) -> Result<(), CreateError>
where
    M: DinocoInsertable,
    V: InsertPayload<M>,
{
    if V::HAS_NESTED && values.iter().any(InsertPayload::dinoco_has_nested_values) {
        return Err(CreateError::InvalidOperation(format!(
            "{method} on `{}` does not insert nested relations or many-to-many links; insert them separately",
            M::TABLE_NAME
        )));
    }

    Ok(())
}

/// The insert of `values` with `conflict` applied: an [`InsertQuery`] that
/// skips conflicts, or an [`UpsertQuery`] that updates the existing rows.
enum ConflictInsert {
    Insert(InsertQuery),
    Upsert(UpsertQuery),
}

impl ConflictInsert {
    fn insert(&self) -> &InsertQuery {
        match self {
            ConflictInsert::Insert(query) => query,
            ConflictInsert::Upsert(query) => &query.insert,
        }
    }

    async fn run<C>(self, client: &C) -> Result<(), CreateError>
    where
        C: crate::MutationExecutor,
    {
        match self {
            ConflictInsert::Insert(query) => client.insert(query).await,
            ConflictInsert::Upsert(query) => client.upsert(query).await,
        }
        .map_err(CreateError::from_database)?;

        Ok(())
    }
}

fn conflict_insert<M, V>(values: Vec<V>, conflict: Conflict) -> Result<ConflictInsert, CreateError>
where
    M: DinocoInsertable,
    V: InsertPayload<M>,
{
    match conflict {
        Conflict::Fail => Ok(ConflictInsert::Insert(owned_insert_query::<M, V>(values, None, None))),
        Conflict::DoNothing(target) => {
            Ok(ConflictInsert::Insert(owned_insert_query::<M, V>(values, None, Some(target))))
        }
        Conflict::DoUpdate(target, mut sets) => {
            for set in &sets {
                let field = set.field();
                if target.contains(&field) {
                    return Err(CreateError::InvalidOperation(format!(
                        "on_conflict_do_update(...) on `{}` can't assign `{field}`: it is a conflict column, which \
                         finds the row to update",
                        M::TABLE_NAME
                    )));
                }
                if matches!(set, ConflictSet::Excluded(_)) && !M::INSERT_FIELDS.contains(&field) {
                    return Err(CreateError::InvalidOperation(format!(
                        "on_conflict_do_update(...) on `{}` can't take `{field}` from the inserted row: the insert \
                         does not write it",
                        M::TABLE_NAME
                    )));
                }
            }
            // `@updated_at` columns are refreshed by the update, as by
            // `update(...)`, unless it assigns them itself.
            if !sets.is_empty() {
                for updated_at in M::UPDATED_AT_FIELDS {
                    if !sets.iter().any(|set| set.field() == updated_at.name) {
                        sets.push(ConflictSet::Update(updated_at.update_set()));
                    }
                }
            }

            let insert = owned_insert_query::<M, V>(values, None, None);
            Ok(ConflictInsert::Upsert(UpsertQuery { insert, target, sets }))
        }
    }
}

/// Inserts `values` with `conflict` applied: each one that collides with an
/// existing row on its target is skipped, or updates that row.
pub(crate) async fn execute_insert_on_conflict<M, V, C>(
    values: Vec<V>,
    conflict: Conflict,
    client: &C,
) -> Result<(), CreateError>
where
    M: DinocoInsertable,
    V: InsertPayload<M>,
    C: crate::MutationExecutor,
{
    reject_nested_on_conflict::<M, V>(&values, conflict.method())?;
    if values.is_empty() {
        return Ok(());
    }

    conflict_insert::<M, V>(values, conflict)?.run(client).await
}

/// [`execute_insert_on_conflict`], then reads back the row every payload landed on (the new row, or the
/// one already there, skipped or updated) projected onto `fields`, in payload order.
pub(crate) async fn execute_insert_or_existing<M, V, R, C>(
    values: Vec<V>,
    conflict: Conflict,
    fields: &'static [&'static str],
    client: &C,
) -> Result<Vec<R>, CreateError>
where
    M: DinocoInsertable,
    V: InsertPayload<M>,
    R: DinocoRowModel,
    C: crate::MutationExecutor,
{
    let method = conflict.method();
    reject_nested_on_conflict::<M, V>(&values, method)?;
    if values.is_empty() {
        return Ok(Vec::new());
    }

    let target = conflict.target();
    let query = conflict_insert::<M, V>(values, conflict)?;
    let mut keys = conflict_keys(query.insert(), target, method)?;
    let expected = keys.len();
    query.run(client).await?;

    let rows = if expected == 1 {
        let mut lookup = FindQuery::new(fields, M::TABLE_NAME, 1, -1);
        lookup.conditions = target
            .iter()
            .zip(keys.pop().unwrap_or_default())
            .map(|(field, value)| FindWhere::Eq(field, value))
            .collect();
        client.query::<R>(lookup).await
    } else {
        client.lookup::<R>(LookupQuery { table: M::TABLE_NAME, fields, key_fields: target, keys }).await
    }
    .map_err(CreateError::from_database)?;

    if rows.len() != expected {
        return Err(CreateError::NotReturned { table: M::TABLE_NAME });
    }

    Ok(rows)
}

/// The `target` values of every row of `query`, which find the row again
/// after the insert. A `NULL` never collides with anything, nor finds it.
fn conflict_keys(
    query: &InsertQuery,
    target: &[&'static str],
    method: &str,
) -> Result<Vec<Vec<DinocoValue>>, CreateError> {
    let positions = target
        .iter()
        .map(|column| {
            query.fields.iter().position(|field| field == column).ok_or_else(|| {
                CreateError::InvalidOperation(format!(
                    "{method} column `{column}` is not inserted into `{}`, so the row can't be read back",
                    query.table
                ))
            })
        })
        .collect::<Result<Vec<_>, _>>()?;

    query
        .rows
        .iter()
        .map(|row| {
            positions
                .iter()
                .map(|&position| match &row[position] {
                    DinocoValue::Null => Err(CreateError::InvalidOperation(format!(
                        "{method} column `{}` of `{}` is NULL, which never conflicts, so the row can't be read back",
                        query.fields[position], query.table
                    ))),
                    value => Ok(value.clone()),
                })
                .collect()
        })
        .collect()
}

pub async fn reload_inserted<M, S, C>(models: &[M], client: &C) -> anyhow::Result<Vec<S>>
where
    M: DinocoInsertable,
    S: DinocoProjection<M> + DinocoRowModel,
    C: crate::MutationExecutor,
{
    let mut result = Vec::with_capacity(models.len());

    for model in models {
        let mut query = FindQuery::new(S::FIELDS, M::TABLE_NAME, 1, -1);
        query.conditions = model.dinoco_insert_identity();

        let mut rows = client.query::<S>(query).await?;

        if let Some(row) = rows.pop() {
            result.push(row);
        }
    }

    Ok(result)
}

/// Single-column counterpart of [`reload_inserted`], backing nested-insert
/// payloads passed through `insert_into::<M>().pluck(...)`.
pub(crate) async fn reload_inserted_field<M, T, C>(
    models: &[M],
    field: &'static str,
    client: &C,
) -> anyhow::Result<Vec<T>>
where
    M: DinocoInsertable,
    PluckValue<T>: DinocoRowModel,
    C: crate::MutationExecutor,
{
    let fields = single_field(field, <M as DinocoEntity>::FIELDS);
    let mut result = Vec::with_capacity(models.len());

    for model in models {
        let mut query = FindQuery::new(fields, M::TABLE_NAME, 1, -1);
        query.conditions = model.dinoco_insert_identity();

        let mut rows = client.query::<PluckValue<T>>(query).await?;

        if let Some(row) = rows.pop() {
            result.push(row.0);
        }
    }

    Ok(result)
}
