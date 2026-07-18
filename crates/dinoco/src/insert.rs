use std::borrow::Borrow;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicU16, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use dinoco_engine::{
    DinocoClient, DinocoEntity, DinocoProjection, DinocoRowModel, DinocoValue, FindQuery, FindWhere, InsertQuery,
};

pub type InsertNestedFuture<'a> = Pin<Box<dyn Future<Output = anyhow::Result<()>> + 'a>>;

pub trait DinocoInsertable: DinocoEntity + Sized {
    const INSERT_FIELDS: &'static [&'static str];

    fn dinoco_insert_values(&self) -> Vec<DinocoValue>;
    fn dinoco_insert_identity(&self) -> Vec<FindWhere>;
}

pub trait InsertPayload<M>
where
    M: DinocoInsertable,
{
    const HAS_NESTED: bool = false;

    fn dinoco_insert_model(&self) -> M;

    fn dinoco_insert_nested<'a>(&'a self, _parent: &'a M, _client: &'a DinocoClient) -> InsertNestedFuture<'a> {
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

    fn dinoco_insert_nested<'a>(&'a self, parent: &'a M, client: &'a DinocoClient) -> InsertNestedFuture<'a> {
        (*self).dinoco_insert_nested(parent, client)
    }
}

pub trait DinocoBelongsTo<P> {
    fn dinoco_bind_parent(&mut self, parent: &P);
}

pub fn new_uuid() -> String {
    uuid::Uuid::now_v7().to_string()
}

pub fn new_snowflake_id() -> i64 {
    static SEQUENCE: AtomicU16 = AtomicU16::new(0);

    let timestamp =
        SystemTime::now().duration_since(UNIX_EPOCH).map(|duration| duration.as_millis() as i64).unwrap_or_default();
    let sequence = (SEQUENCE.fetch_add(1, Ordering::Relaxed) & 0x0fff) as i64;

    (timestamp << 12) | sequence
}

pub async fn execute_insert_payloads<M, V, B>(values: &[B], client: &DinocoClient) -> anyhow::Result<Vec<M>>
where
    M: DinocoInsertable + DinocoProjection<M> + DinocoRowModel,
    V: InsertPayload<M>,
    B: Borrow<V>,
{
    let mut models = values.iter().map(|value| value.borrow().dinoco_insert_model()).collect::<Vec<_>>();

    if !V::HAS_NESTED {
        execute_insert_models::<M>(&models, client).await?;
        return Ok(models);
    }

    let inserted = execute_insert_models_returning::<M, M>(&models, client).await?;

    for (payload, model) in values.iter().zip(inserted.iter()) {
        payload.borrow().dinoco_insert_nested(model, client).await?;
    }

    models.clear();

    Ok(inserted)
}

pub(crate) async fn execute_insert_payloads_returning<M, V, B>(
    values: &[B],
    client: &DinocoClient,
) -> anyhow::Result<Vec<M>>
where
    M: DinocoInsertable + DinocoProjection<M> + DinocoRowModel,
    V: InsertPayload<M>,
    B: Borrow<V>,
{
    let models = values.iter().map(|value| value.borrow().dinoco_insert_model()).collect::<Vec<_>>();
    let inserted = execute_insert_models_returning::<M, M>(&models, client).await?;

    if V::HAS_NESTED {
        for (payload, model) in values.iter().zip(inserted.iter()) {
            payload.borrow().dinoco_insert_nested(model, client).await?;
        }
    }

    Ok(inserted)
}

pub async fn execute_insert_models<M>(models: &[M], client: &DinocoClient) -> anyhow::Result<usize>
where
    M: DinocoInsertable,
{
    if models.is_empty() {
        return Ok(0);
    }

    let rows = models.iter().map(DinocoInsertable::dinoco_insert_values).collect::<Vec<_>>();
    let query = InsertQuery { table: M::TABLE_NAME, fields: M::INSERT_FIELDS.to_vec(), rows, returning: None };

    client.backend.insert(query).await
}

pub async fn execute_insert_models_returning<M, S>(models: &[M], client: &DinocoClient) -> anyhow::Result<Vec<S>>
where
    M: DinocoInsertable,
    S: DinocoProjection<M> + DinocoRowModel,
{
    if models.is_empty() {
        return Ok(Vec::new());
    }

    let rows = models.iter().map(DinocoInsertable::dinoco_insert_values).collect::<Vec<_>>();
    let query =
        InsertQuery { table: M::TABLE_NAME, fields: M::INSERT_FIELDS.to_vec(), rows, returning: Some(S::FIELDS) };

    client.backend.insert_returning::<S>(query).await
}

pub async fn reload_inserted<M, S>(models: &[M], client: &DinocoClient) -> anyhow::Result<Vec<S>>
where
    M: DinocoInsertable,
    S: DinocoProjection<M> + DinocoRowModel,
{
    let mut result = Vec::with_capacity(models.len());

    for model in models {
        let mut query = FindQuery::new(S::FIELDS, M::TABLE_NAME, 1, -1);
        query.conditions = model.dinoco_insert_identity();

        let mut rows = client.backend.query::<S>(query).await?;

        if let Some(row) = rows.pop() {
            result.push(row);
        }
    }

    Ok(result)
}
