use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::marker::PhantomData;
use std::pin::Pin;

use dinoco_engine::{
    DinocoClient, DinocoEntity, DinocoMysql, DinocoPostgres, DinocoProjection, DinocoRowModel, DinocoSqlite,
    DinocoValue, FindOrderBy, FindQuery, FindWhere, ManyToManyRelationQuery, RelationBatchQuery, RelationJoinQuery,
    WhereComplex,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IncludeStrategy {
    DataLoader,
    LeftJoin,
}

pub(crate) type IncludeApplier<S> = Box<dyn FnOnce(&mut [S]) + Send>;
pub(crate) type IncludeLoaderFuture<'a, S> =
    Pin<Box<dyn Future<Output = anyhow::Result<IncludeApplier<S>>> + Send + 'a>>;

pub trait IncludeLoader<S>: Send + Sync {
    fn load_applier<'a>(
        &'a self,
        client: &'a DinocoClient,
        parents: &'a [S],
        read_primary: bool,
    ) -> IncludeLoaderFuture<'a, S>;
}

pub(crate) async fn load_includes<S>(
    includes: Vec<Box<dyn IncludeLoader<S>>>,
    client: &DinocoClient,
    items: &mut [S],
    read_primary: bool,
) -> anyhow::Result<()> {
    let appliers =
        futures::future::try_join_all(includes.iter().map(|include| include.load_applier(client, items, read_primary)))
            .await?;

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
    complex_where: bool,
    many_to_many: Option<ManyToManyMetadata>,
    marker: PhantomData<fn() -> (M, C, CS)>,
}

#[derive(Clone, Copy)]
struct ManyToManyMetadata {
    join_table: &'static str,
    join_parent_field: &'static str,
    join_child_field: &'static str,
}

pub struct BelongsTo<M, C, CS = C> {
    relation: &'static str,
    parent_field: &'static str,
    child_field: &'static str,
    query: FindQuery,
    includes: Vec<Box<dyn IncludeLoader<CS>>>,
    complex_where: bool,
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
            complex_where: false,
            many_to_many: None,
            marker: PhantomData,
        }
    }

    pub fn many_to_many(
        relation: &'static str,
        parent_field: &'static str,
        child_field: &'static str,
        join_table: &'static str,
        join_parent_field: &'static str,
        join_child_field: &'static str,
    ) -> Self {
        Self {
            relation,
            parent_field,
            child_field,
            query: FindQuery::new(C::FIELDS, C::TABLE_NAME, -1, -1),
            includes: Vec::new(),
            complex_where: false,
            many_to_many: Some(ManyToManyMetadata { join_table, join_parent_field, join_child_field }),
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
            complex_where: false,
            marker: PhantomData,
        }
    }
}

impl<M, C, CS> HasMany<M, C, CS>
where
    C: DinocoEntity,
    CS: DinocoRowModel,
{
    pub fn strategy(&self) -> IncludeStrategy {
        IncludeStrategy::DataLoader
    }

    pub fn where_<F>(mut self, callback: F) -> Self
    where
        F: FnOnce(C::Where) -> FindWhere,
    {
        if !self.complex_where {
            self.query.conditions.push(callback(C::Where::default()));
        }

        self
    }

    pub fn where_complex<F>(mut self, callback: F) -> Self
    where
        F: FnOnce(C::Where, WhereComplex) -> FindWhere,
    {
        self.query.conditions = vec![callback(C::Where::default(), WhereComplex)];
        self.complex_where = true;

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
            complex_where: self.complex_where,
            many_to_many: self.many_to_many,
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
    CS: DinocoRowModel,
{
    pub fn strategy(&self) -> IncludeStrategy {
        IncludeStrategy::LeftJoin
    }

    pub fn where_<F>(mut self, callback: F) -> Self
    where
        F: FnOnce(C::Where) -> FindWhere,
    {
        if !self.complex_where {
            self.query.conditions.push(callback(C::Where::default()));
        }

        self
    }

    pub fn where_complex<F>(mut self, callback: F) -> Self
    where
        F: FnOnce(C::Where, WhereComplex) -> FindWhere,
    {
        self.query.conditions = vec![callback(C::Where::default(), WhereComplex)];
        self.complex_where = true;

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
            complex_where: self.complex_where,
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
    S: DinocoRelationValue + DinocoRelationApply<CS> + Sync + 'static,
    C: DinocoEntity + DinocoRowModel + 'static,
    CS: DinocoProjection<C> + DinocoRelationValue + 'static,
{
    fn into_include_loader(self) -> Box<dyn IncludeLoader<S>> {
        Box::new(self)
    }
}

impl<M, S, C, CS> IntoIncludeLoader<M, S> for BelongsTo<M, C, CS>
where
    M: DinocoEntity + 'static,
    S: DinocoRelationValue + DinocoRelationApply<CS> + Sync + 'static,
    C: DinocoEntity + DinocoRowModel + 'static,
    CS: DinocoProjection<C> + DinocoRelationValue + 'static,
{
    fn into_include_loader(self) -> Box<dyn IncludeLoader<S>> {
        Box::new(self)
    }
}

impl<M, S, C, CS> IncludeLoader<S> for HasMany<M, C, CS>
where
    S: DinocoRelationValue + DinocoRelationApply<CS> + Sync + 'static,
    C: DinocoEntity + DinocoRowModel,
    CS: DinocoProjection<C> + DinocoRelationValue + 'static,
{
    fn load_applier<'a>(
        &'a self,
        client: &'a DinocoClient,
        parents: &'a [S],
        read_primary: bool,
    ) -> IncludeLoaderFuture<'a, S> {
        Box::pin(async move {
            let keys = unique_relation_values(parents, self.parent_field);

            if keys.is_empty() {
                return Ok(noop_include_applier());
            }

            let child_rows = if let Some(many_to_many) = self.many_to_many {
                let query = ManyToManyRelationQuery {
                    query: self.query.clone(),
                    join_table: many_to_many.join_table,
                    parent_field: self.parent_field,
                    child_field: self.child_field,
                    join_parent_field: many_to_many.join_parent_field,
                    join_child_field: many_to_many.join_child_field,
                    key_count: keys.len(),
                };
                client
                    .read_backend(read_primary)
                    .query_many_to_many_relation::<RelationManyRow<C, CS>>(query, &keys)
                    .await?
            } else {
                let mut find_query = self.query.clone();
                find_query.conditions.push(FindWhere::Batch(self.child_field, keys));
                let query = RelationBatchQuery { query: find_query, relation_key_field: self.child_field };
                client.read_backend(read_primary).query_relation_batch::<RelationManyRow<C, CS>>(query).await?
            };
            let relation_keys = child_rows.iter().map(|row| relation_key(&row.key)).collect::<Vec<_>>();
            let mut children = child_rows.into_iter().map(|row| row.item).collect::<Vec<_>>();
            let appliers = futures::future::try_join_all(
                self.includes.iter().map(|include| include.load_applier(client, &children, read_primary)),
            )
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

            Ok(Box::new(move |parents: &mut [S]| {
                for parent in parents {
                    let values = parent
                        .dinoco_relation_value(parent_field)
                        .and_then(|key| grouped.remove(&relation_key(&key)))
                        .unwrap_or_default();

                    parent.dinoco_apply_many(relation, values);
                }
            }) as IncludeApplier<S>)
        })
    }
}

impl<M, S, C, CS> IncludeLoader<S> for BelongsTo<M, C, CS>
where
    M: DinocoEntity,
    S: DinocoRelationValue + DinocoRelationApply<CS> + Sync + 'static,
    C: DinocoEntity + DinocoRowModel,
    CS: DinocoProjection<C> + DinocoRelationValue + 'static,
{
    fn load_applier<'a>(
        &'a self,
        client: &'a DinocoClient,
        parents: &'a [S],
        read_primary: bool,
    ) -> IncludeLoaderFuture<'a, S> {
        Box::pin(async move {
            let keys = unique_relation_values(parents, self.parent_field);

            if keys.is_empty() {
                return Ok(noop_include_applier());
            }

            let query = RelationJoinQuery {
                query: self.query.clone(),
                parent_table: M::TABLE_NAME,
                child_table: C::TABLE_NAME,
                parent_field: self.parent_field,
                child_field: self.child_field,
                key_count: keys.len(),
            };

            let child_rows = client
                .read_backend(read_primary)
                .query_relation_join_optional::<RelationOneRow<C, CS>>(query, &keys)
                .await?;
            let child_pairs = child_rows
                .into_iter()
                .filter_map(|row| row.item.map(|item| (relation_key(&row.key), item)))
                .collect::<Vec<_>>();
            let relation_keys = child_pairs.iter().map(|(key, _)| key.clone()).collect::<Vec<_>>();
            let mut children = child_pairs.into_iter().map(|(_, item)| item).collect::<Vec<_>>();
            let appliers = futures::future::try_join_all(
                self.includes.iter().map(|include| include.load_applier(client, &children, read_primary)),
            )
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

            Ok(Box::new(move |parents: &mut [S]| {
                for parent in parents {
                    let value =
                        parent.dinoco_relation_value(parent_field).and_then(|key| grouped.remove(&relation_key(&key)));

                    parent.dinoco_apply_one(relation, value);
                }
            }) as IncludeApplier<S>)
        })
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

impl<C, CS> DinocoPostgres for RelationManyRow<C, CS>
where
    C: DinocoEntity,
    CS: DinocoProjection<C>,
{
    fn from_deadpool_posgres_row(row: &dinoco_engine::DeadpoolPostgresRow) -> Option<Self> {
        Some(Self {
            item: CS::from_deadpool_posgres_row(row)?,
            key: row.try_get(CS::FIELDS.len()).ok()?,
            marker: PhantomData,
        })
    }

    fn from_deadpool_postgres_row(row: &dinoco_engine::DeadpoolPostgresRow) -> Option<Self> {
        Self::from_deadpool_posgres_row(row)
    }

    fn from_postgres_row(row: &dinoco_engine::PostgresRow) -> Option<Self> {
        Some(Self { item: CS::from_postgres_row(row)?, key: row.try_get(CS::FIELDS.len()).ok()?, marker: PhantomData })
    }
}

impl<C, CS> DinocoMysql for RelationManyRow<C, CS>
where
    C: DinocoEntity,
    CS: DinocoProjection<C>,
{
    fn from_mysql_row(row: &dinoco_engine::MysqlRow) -> Option<Self> {
        let mut row = row.clone();

        Some(Self { item: CS::from_mysql_row(&row)?, key: row.take(CS::FIELDS.len())?, marker: PhantomData })
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

impl<C, CS> DinocoPostgres for RelationOneRow<C, CS>
where
    C: DinocoEntity,
    CS: DinocoProjection<C>,
{
    fn from_deadpool_posgres_row(row: &dinoco_engine::DeadpoolPostgresRow) -> Option<Self> {
        let child_key_offset = CS::FIELDS.len();
        let relation_key_offset = child_key_offset + 1;
        let child_key = row.try_get::<_, Option<DinocoValue>>(child_key_offset).ok()?;

        Some(Self {
            item: if child_key.is_some() { Some(CS::from_deadpool_posgres_row(row)?) } else { None },
            key: row.try_get(relation_key_offset).ok()?,
            marker: PhantomData,
        })
    }

    fn from_deadpool_postgres_row(row: &dinoco_engine::DeadpoolPostgresRow) -> Option<Self> {
        Self::from_deadpool_posgres_row(row)
    }

    fn from_postgres_row(row: &dinoco_engine::PostgresRow) -> Option<Self> {
        let child_key_offset = CS::FIELDS.len();
        let relation_key_offset = child_key_offset + 1;
        let child_key = row.try_get::<_, Option<DinocoValue>>(child_key_offset).ok()?;

        Some(Self {
            item: if child_key.is_some() { Some(CS::from_postgres_row(row)?) } else { None },
            key: row.try_get(relation_key_offset).ok()?,
            marker: PhantomData,
        })
    }
}

impl<C, CS> DinocoMysql for RelationOneRow<C, CS>
where
    C: DinocoEntity,
    CS: DinocoProjection<C>,
{
    fn from_mysql_row(row: &dinoco_engine::MysqlRow) -> Option<Self> {
        let child_key_offset = CS::FIELDS.len();
        let relation_key_offset = child_key_offset + 1;
        let mut row = row.clone();
        let child_key = row.take::<Option<DinocoValue>, _>(child_key_offset)?;

        Some(Self {
            item: if child_key.is_some() { Some(CS::from_mysql_row(&row)?) } else { None },
            key: row.take(relation_key_offset)?,
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
    Json(String),
    DateTime(String),
    Date(String),
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
        DinocoValue::Json(value) => RelationKey::Json(value.to_string()),
        DinocoValue::DateTime(value) => RelationKey::DateTime(value.to_rfc3339()),
        DinocoValue::Date(value) => RelationKey::Date(value.to_string()),
    }
}

fn unique_relation_values<S>(items: &[S], field: &'static str) -> Vec<DinocoValue>
where
    S: DinocoRelationValue,
{
    let mut seen = HashSet::<RelationKey>::new();
    let mut values = Vec::new();

    for value in items.iter().filter_map(|item| item.dinoco_relation_value(field)) {
        let key = relation_key(&value);

        if seen.insert(key) {
            values.push(value);
        }
    }

    values
}

fn noop_include_applier<S>() -> IncludeApplier<S> {
    Box::new(|_| {})
}
