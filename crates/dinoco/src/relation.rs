use std::collections::HashMap;
use std::marker::PhantomData;

use async_trait::async_trait;
use dinoco_engine::{
    DinocoClient, DinocoEntity, DinocoProjection, DinocoSqlite, DinocoValue, FindOrderBy, FindQuery, FindWhere,
    RelationBatchQuery, RelationJoinQuery,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IncludeStrategy {
    DataLoader,
    LeftJoin,
}

pub(crate) type IncludeApplier<S> = Box<dyn FnOnce(&mut [S])>;

#[async_trait(?Send)]
pub trait IncludeLoader<S> {
    async fn load(&self, client: &DinocoClient, parents: &mut [S]) -> anyhow::Result<()> {
        let apply = self.load_applier(client, parents).await?;
        apply(parents);

        Ok(())
    }

    async fn load_applier(&self, client: &DinocoClient, parents: &[S]) -> anyhow::Result<IncludeApplier<S>>;
}

pub(crate) async fn load_includes<S>(
    includes: Vec<Box<dyn IncludeLoader<S>>>,
    client: &DinocoClient,
    items: &mut [S],
) -> anyhow::Result<()> {
    let appliers =
        futures::future::try_join_all(includes.iter().map(|include| include.load_applier(client, items))).await?;

    for apply in appliers {
        apply(items);
    }

    Ok(())
}

pub trait IntoIncludeLoader<M, S> {
    fn into_include_loader(self) -> Box<dyn IncludeLoader<S>>;
}

pub trait DinocoRelationValue {
    fn dinoco_relation_value(&self, field: &'static str) -> Option<DinocoValue>;
}

pub trait DinocoRelationApply<C> {
    fn dinoco_apply_many(&mut self, relation: &'static str, values: Vec<C>);
    fn dinoco_apply_one(&mut self, relation: &'static str, value: Option<C>);
}

pub struct HasMany<M, C, CS = C> {
    relation: &'static str,
    parent_field: &'static str,
    child_field: &'static str,
    query: FindQuery,
    includes: Vec<Box<dyn IncludeLoader<CS>>>,
    marker: PhantomData<fn() -> (M, C, CS)>,
}

pub struct BelongsTo<M, C, CS = C> {
    relation: &'static str,
    parent_field: &'static str,
    child_field: &'static str,
    query: FindQuery,
    includes: Vec<Box<dyn IncludeLoader<CS>>>,
    marker: PhantomData<fn() -> (M, C, CS)>,
}

impl<M, C> HasMany<M, C, C>
where
    C: DinocoEntity,
{
    pub fn new(relation: &'static str, parent_field: &'static str, child_field: &'static str) -> Self {
        Self {
            relation,
            parent_field,
            child_field,
            query: FindQuery::new(C::FIELDS, C::TABLE_NAME, -1, -1),
            includes: Vec::new(),
            marker: PhantomData,
        }
    }
}

impl<M, C> BelongsTo<M, C, C>
where
    C: DinocoEntity,
{
    pub fn new(relation: &'static str, parent_field: &'static str, child_field: &'static str) -> Self {
        Self {
            relation,
            parent_field,
            child_field,
            query: FindQuery::new(C::FIELDS, C::TABLE_NAME, -1, -1),
            includes: Vec::new(),
            marker: PhantomData,
        }
    }
}

impl<M, C, CS> HasMany<M, C, CS>
where
    C: DinocoEntity,
    CS: DinocoSqlite,
{
    pub fn strategy(&self) -> IncludeStrategy {
        IncludeStrategy::DataLoader
    }

    pub fn where_<F>(mut self, callback: F) -> Self
    where
        F: FnOnce(C::Where) -> FindWhere,
    {
        self.query.conditions.push(callback(C::Where::default()));

        self
    }

    pub fn select<NCS>(mut self) -> HasMany<M, C, NCS>
    where
        NCS: DinocoProjection<C>,
    {
        self.query.fields = NCS::FIELDS;

        HasMany {
            relation: self.relation,
            parent_field: self.parent_field,
            child_field: self.child_field,
            query: self.query,
            includes: Vec::new(),
            marker: PhantomData,
        }
    }

    pub fn order_by<F>(mut self, closure: F) -> Self
    where
        F: FnOnce(C::OrderBy) -> FindOrderBy,
    {
        self.query.order_by = Some(closure(C::OrderBy::default()));

        self
    }

    pub fn includes<F, I>(mut self, closure: F) -> Self
    where
        F: FnOnce(C::Include) -> I,
        I: IntoIncludeLoader<C, CS>,
    {
        self.includes.push(closure(C::Include::default()).into_include_loader());

        self
    }

    pub fn take(mut self, value: i32) -> Self {
        self.query.limit = value;

        self
    }

    pub fn skip(mut self, value: i32) -> Self {
        self.query.skip = value;

        self
    }
}

impl<M, C, CS> BelongsTo<M, C, CS>
where
    C: DinocoEntity,
    CS: DinocoSqlite,
{
    pub fn strategy(&self) -> IncludeStrategy {
        IncludeStrategy::LeftJoin
    }

    pub fn where_<F>(mut self, callback: F) -> Self
    where
        F: FnOnce(C::Where) -> FindWhere,
    {
        self.query.conditions.push(callback(C::Where::default()));

        self
    }

    pub fn select<NCS>(mut self) -> BelongsTo<M, C, NCS>
    where
        NCS: DinocoProjection<C>,
    {
        self.query.fields = NCS::FIELDS;

        BelongsTo {
            relation: self.relation,
            parent_field: self.parent_field,
            child_field: self.child_field,
            query: self.query,
            includes: Vec::new(),
            marker: PhantomData,
        }
    }

    pub fn order_by<F>(mut self, closure: F) -> Self
    where
        F: FnOnce(C::OrderBy) -> FindOrderBy,
    {
        self.query.order_by = Some(closure(C::OrderBy::default()));

        self
    }

    pub fn includes<F, I>(mut self, closure: F) -> Self
    where
        F: FnOnce(C::Include) -> I,
        I: IntoIncludeLoader<C, CS>,
    {
        self.includes.push(closure(C::Include::default()).into_include_loader());

        self
    }

    pub fn take(mut self, value: i32) -> Self {
        self.query.limit = value;

        self
    }

    pub fn skip(mut self, value: i32) -> Self {
        self.query.skip = value;

        self
    }
}

impl<M, S, C, CS> IntoIncludeLoader<M, S> for HasMany<M, C, CS>
where
    M: DinocoEntity + 'static,
    S: DinocoRelationValue + DinocoRelationApply<CS> + 'static,
    C: DinocoEntity + DinocoSqlite + 'static,
    CS: DinocoProjection<C> + DinocoRelationValue + 'static,
{
    fn into_include_loader(self) -> Box<dyn IncludeLoader<S>> {
        Box::new(self)
    }
}

impl<M, S, C, CS> IntoIncludeLoader<M, S> for BelongsTo<M, C, CS>
where
    M: DinocoEntity + 'static,
    S: DinocoRelationValue + DinocoRelationApply<CS> + 'static,
    C: DinocoEntity + DinocoSqlite + 'static,
    CS: DinocoProjection<C> + DinocoRelationValue + 'static,
{
    fn into_include_loader(self) -> Box<dyn IncludeLoader<S>> {
        Box::new(self)
    }
}

#[async_trait(?Send)]
impl<M, S, C, CS> IncludeLoader<S> for HasMany<M, C, CS>
where
    S: DinocoRelationValue + DinocoRelationApply<CS> + 'static,
    C: DinocoEntity + DinocoSqlite,
    CS: DinocoProjection<C> + DinocoRelationValue + 'static,
{
    async fn load_applier(&self, client: &DinocoClient, parents: &[S]) -> anyhow::Result<IncludeApplier<S>> {
        let keys =
            parents.iter().filter_map(|parent| parent.dinoco_relation_value(self.parent_field)).collect::<Vec<_>>();

        if keys.is_empty() {
            return Ok(Box::new(|_| {}));
        }

        let mut find_query = self.query.clone();
        find_query.conditions.push(FindWhere::Batch(self.child_field, keys));
        let query = RelationBatchQuery { query: find_query, relation_key_field: self.child_field };

        let child_rows = client.backend.query_relation_batch::<RelationManyRow<C, CS>>(query).await?;
        let relation_keys = child_rows.iter().map(|row| relation_key(&row.key)).collect::<Vec<_>>();
        let mut children = child_rows.into_iter().map(|row| row.item).collect::<Vec<_>>();
        let appliers =
            futures::future::try_join_all(self.includes.iter().map(|include| include.load_applier(client, &children)))
                .await?;

        for apply in appliers {
            apply(&mut children);
        }

        let mut grouped = HashMap::<RelationKey, Vec<CS>>::new();

        for (key, child) in relation_keys.into_iter().zip(children.into_iter()) {
            grouped.entry(key).or_default().push(child);
        }

        let relation = self.relation;
        let parent_field = self.parent_field;

        Ok(Box::new(move |parents| {
            for parent in parents {
                let values = parent
                    .dinoco_relation_value(parent_field)
                    .and_then(|key| grouped.remove(&relation_key(&key)))
                    .unwrap_or_default();

                parent.dinoco_apply_many(relation, values);
            }
        }))
    }
}

#[async_trait(?Send)]
impl<M, S, C, CS> IncludeLoader<S> for BelongsTo<M, C, CS>
where
    M: DinocoEntity,
    S: DinocoRelationValue + DinocoRelationApply<CS> + 'static,
    C: DinocoEntity + DinocoSqlite,
    CS: DinocoProjection<C> + DinocoRelationValue + 'static,
{
    async fn load_applier(&self, client: &DinocoClient, parents: &[S]) -> anyhow::Result<IncludeApplier<S>> {
        let keys =
            parents.iter().filter_map(|parent| parent.dinoco_relation_value(self.parent_field)).collect::<Vec<_>>();

        if keys.is_empty() {
            return Ok(Box::new(|_| {}));
        }

        let query = RelationJoinQuery {
            fields: self.query.fields,
            parent_table: M::TABLE_NAME,
            child_table: C::TABLE_NAME,
            parent_field: self.parent_field,
            child_field: self.child_field,
            key_count: keys.len(),
        };

        let child_rows = client.backend.query_relation_join_optional::<RelationOneRow<C, CS>>(query, &keys).await?;
        let child_pairs = child_rows
            .into_iter()
            .filter_map(|row| row.item.map(|item| (relation_key(&row.key), item)))
            .collect::<Vec<_>>();
        let relation_keys = child_pairs.iter().map(|(key, _)| key.clone()).collect::<Vec<_>>();
        let mut children = child_pairs.into_iter().map(|(_, item)| item).collect::<Vec<_>>();
        let appliers =
            futures::future::try_join_all(self.includes.iter().map(|include| include.load_applier(client, &children)))
                .await?;

        for apply in appliers {
            apply(&mut children);
        }

        let mut grouped = HashMap::<RelationKey, CS>::new();

        for (key, child) in relation_keys.into_iter().zip(children.into_iter()) {
            grouped.insert(key, child);
        }

        let relation = self.relation;
        let parent_field = self.parent_field;

        Ok(Box::new(move |parents| {
            for parent in parents {
                let value =
                    parent.dinoco_relation_value(parent_field).and_then(|key| grouped.remove(&relation_key(&key)));

                parent.dinoco_apply_one(relation, value);
            }
        }))
    }
}

struct RelationManyRow<C, CS> {
    item: CS,
    key: DinocoValue,
    marker: PhantomData<C>,
}

impl<C, CS> DinocoSqlite for RelationManyRow<C, CS>
where
    C: DinocoEntity,
    CS: DinocoProjection<C>,
{
    fn from_sqlite_row(row: &dinoco_engine::SqliteRow<'_>) -> Option<Self> {
        Some(Self { item: CS::from_sqlite_row(row)?, key: row.get(CS::FIELDS.len()).ok()?, marker: PhantomData })
    }
}

struct RelationOneRow<C, CS> {
    item: Option<CS>,
    key: DinocoValue,
    marker: PhantomData<C>,
}

impl<C, CS> DinocoSqlite for RelationOneRow<C, CS>
where
    C: DinocoEntity,
    CS: DinocoProjection<C>,
{
    fn from_sqlite_row(row: &dinoco_engine::SqliteRow<'_>) -> Option<Self> {
        let child_key_offset = CS::FIELDS.len();
        let relation_key_offset = child_key_offset + 1;
        let child_key = row.get::<_, Option<DinocoValue>>(child_key_offset).ok()?;

        Some(Self {
            item: if child_key.is_some() { Some(CS::from_sqlite_row(row)?) } else { None },
            key: row.get(relation_key_offset).ok()?,
            marker: PhantomData,
        })
    }
}

#[derive(Clone, PartialEq, Eq, Hash)]
enum RelationKey {
    Null,
    Integer(i64),
    Float(u64),
    String(String),
    Enum(String, String),
    Boolean(bool),
    Bytes(Vec<u8>),
}

fn relation_key(value: &DinocoValue) -> RelationKey {
    match value {
        DinocoValue::Null => RelationKey::Null,
        DinocoValue::Integer(value) => RelationKey::Integer(*value),
        DinocoValue::Float(value) => RelationKey::Float(value.to_bits()),
        DinocoValue::String(value) => RelationKey::String(value.clone()),
        DinocoValue::Enum(name, value) => RelationKey::Enum(name.clone(), value.clone()),
        DinocoValue::Boolean(value) => RelationKey::Boolean(*value),
        DinocoValue::Bytes(value) => RelationKey::Bytes(value.clone()),
    }
}
