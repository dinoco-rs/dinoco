use std::borrow::Borrow;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicU16, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use dinoco_engine::{
    DinocoEntity, DinocoProjection, DinocoRowModel, DinocoValue, FindQuery, FindWhere, InsertQuery, PluckValue,
};

use crate::single_field;

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
    let query = InsertQuery { table: M::TABLE_NAME, fields: M::INSERT_FIELDS.to_vec(), rows, returning: None };

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
    let query =
        InsertQuery { table: M::TABLE_NAME, fields: M::INSERT_FIELDS.to_vec(), rows, returning: Some(S::FIELDS) };

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

    client.insert(owned_insert_query::<M, V>(values, None)).await
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

    client.insert_returning::<S>(owned_insert_query::<M, V>(values, Some(S::FIELDS))).await
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
    let rows = client.insert_returning::<PluckValue<T>>(owned_insert_query::<M, V>(values, Some(fields))).await?;

    Ok(rows.into_iter().map(|value| value.0).collect())
}

fn owned_insert_query<M, V>(values: Vec<V>, returning: Option<&'static [&'static str]>) -> InsertQuery
where
    M: DinocoInsertable,
    V: InsertPayload<M>,
{
    let rows = values.into_iter().map(|value| value.dinoco_into_insert_model().dinoco_into_insert_values()).collect();

    InsertQuery { table: M::TABLE_NAME, fields: M::INSERT_FIELDS.to_vec(), rows, returning }
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
