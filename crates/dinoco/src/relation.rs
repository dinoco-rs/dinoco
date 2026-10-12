use std::future::Future;
use std::marker::PhantomData;
use std::pin::Pin;

use dinoco_engine::{
    DinocoEntity, DinocoProjection, DinocoRowModel, DinocoValue, FindOrderBy, FindQuery, FindWhere,
    ManyToManyRelationQuery, RelationOccurrenceQuery, WhereComplex,
};

use crate::ReadTarget;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IncludeStrategy {
    DataLoader,
    LeftJoin,
}

pub(crate) type IncludeApplier<S> = Box<dyn FnOnce(&mut [S]) + Send>;
pub(crate) type IncludeLoaderFuture<'a, S> =
    Pin<Box<dyn Future<Output = anyhow::Result<IncludeApplier<S>>> + Send + 'a>>;
type RelationMarker<M, C, CS> = PhantomData<fn() -> (M, C, CS)>;

pub trait IncludeLoader<S>: Send + Sync {
    fn load_applier<'a>(&'a self, target: &'a ReadTarget<'a>, parents: &'a [S]) -> IncludeLoaderFuture<'a, S>;
}

pub(crate) async fn load_includes<S>(
    includes: Vec<Box<dyn IncludeLoader<S>>>,
    target: &ReadTarget<'_>,
    items: &mut [S],
) -> anyhow::Result<()> {
    let appliers =
        futures_util::future::try_join_all(includes.iter().map(|include| include.load_applier(target, items))).await?;

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
    marker: RelationMarker<M, C, CS>,
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
    marker: RelationMarker<M, C, CS>,
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
    fn load_applier<'a>(&'a self, target: &'a ReadTarget<'a>, parents: &'a [S]) -> IncludeLoaderFuture<'a, S> {
        Box::pin(async move {
            let (keys, has_key) = relation_keys(parents, self.parent_field);

            if keys.is_empty() {
                return Ok(noop_include_applier());
            }
            let key_count = keys.len();

            let rows = if let Some(many_to_many) = self.many_to_many {
                let query = ManyToManyRelationQuery {
                    query: self.query.clone(),
                    join_table: many_to_many.join_table,
                    parent_field: self.parent_field,
                    child_field: self.child_field,
                    join_parent_field: many_to_many.join_parent_field,
                    join_child_field: many_to_many.join_child_field,
                    key_count,
                };
                target.query_many_to_many_relation::<RelationOccurrenceRow<C, CS>>(query, keys).await?
            } else {
                let query =
                    RelationOccurrenceQuery { query: self.query.clone(), child_field: self.child_field, key_count };
                target.query_relation_occurrences::<RelationOccurrenceRow<C, CS>>(query, keys).await?
            };
            let (ordinals, children) = load_nested_includes(&self.includes, target, rows).await?;

            // Every parent's children, in one exactly sized vector each.
            let mut counts = vec![0usize; key_count];
            for &ordinal in &ordinals {
                if let Some(count) = counts.get_mut(ordinal) {
                    *count += 1;
                }
            }
            let mut grouped = counts.into_iter().map(Vec::with_capacity).collect::<Vec<Vec<CS>>>();
            for (ordinal, child) in ordinals.into_iter().zip(children) {
                if let Some(values) = grouped.get_mut(ordinal) {
                    values.push(child);
                }
            }

            let relation = self.relation;
            Ok(Box::new(move |parents: &mut [S]| {
                let mut grouped = grouped.into_iter();

                for (parent, has_key) in parents.iter_mut().zip(has_key) {
                    let values = if has_key { grouped.next().unwrap_or_default() } else { Vec::new() };
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
    fn load_applier<'a>(&'a self, target: &'a ReadTarget<'a>, parents: &'a [S]) -> IncludeLoaderFuture<'a, S> {
        Box::pin(async move {
            let (keys, has_key) = relation_keys(parents, self.parent_field);

            if keys.is_empty() {
                return Ok(noop_include_applier());
            }

            let key_count = keys.len();
            let query = RelationOccurrenceQuery { query: self.query.clone(), child_field: self.child_field, key_count };
            let rows = target.query_relation_occurrences::<RelationOccurrenceRow<C, CS>>(query, keys).await?;
            let (ordinals, children) = load_nested_includes(&self.includes, target, rows).await?;

            // The last row matched for each parent wins.
            let mut matched = (0..key_count).map(|_| None).collect::<Vec<Option<CS>>>();
            for (ordinal, child) in ordinals.into_iter().zip(children) {
                if let Some(slot) = matched.get_mut(ordinal) {
                    *slot = Some(child);
                }
            }

            let relation = self.relation;
            Ok(Box::new(move |parents: &mut [S]| {
                let mut matched = matched.into_iter();

                for (parent, has_key) in parents.iter_mut().zip(has_key) {
                    let value = if has_key { matched.next().flatten() } else { None };
                    parent.dinoco_apply_one(relation, value);
                }
            }) as IncludeApplier<S>)
        })
    }
}

/// Splits relation rows into their parent ordinals and items, then loads the
/// items' own includes.
async fn load_nested_includes<C, CS>(
    includes: &[Box<dyn IncludeLoader<CS>>],
    target: &ReadTarget<'_>,
    rows: Vec<RelationOccurrenceRow<C, CS>>,
) -> anyhow::Result<(Vec<usize>, Vec<CS>)> {
    let mut ordinals = Vec::with_capacity(rows.len());
    let mut children = Vec::with_capacity(rows.len());
    for row in rows {
        ordinals.push(row.ordinal);
        children.push(row.item);
    }

    if !includes.is_empty() {
        let appliers =
            futures_util::future::try_join_all(includes.iter().map(|include| include.load_applier(target, &children)))
                .await?;
        for apply in appliers {
            apply(&mut children);
        }
    }

    Ok((ordinals, children))
}

struct RelationOccurrenceRow<C, CS> {
    item: CS,
    ordinal: usize,
    marker: PhantomData<C>,
}

dinoco_engine::__dinoco_sqlite! {
    impl<C, CS> dinoco_engine::DinocoSqlite for RelationOccurrenceRow<C, CS>
    where
        C: DinocoEntity,
        CS: DinocoProjection<C>,
    {
        fn from_sqlite_row(row: &dinoco_engine::SqliteRow<'_>) -> Option<Self> {
            Some(Self {
                item: CS::from_sqlite_row(row)?,
                ordinal: row.get::<_, i64>(CS::FIELDS.len() + 1).ok()?.try_into().ok()?,
                marker: PhantomData,
            })
        }
    }
}

dinoco_engine::__dinoco_postgres! {
    impl<C, CS> dinoco_engine::DinocoPostgres for RelationOccurrenceRow<C, CS>
    where
        C: DinocoEntity,
        CS: DinocoProjection<C>,
    {
        fn from_deadpool_posgres_row(row: &dinoco_engine::DeadpoolPostgresRow) -> Option<Self> {
            Self::from_postgres_row(row)
        }

        fn from_deadpool_postgres_row(row: &dinoco_engine::DeadpoolPostgresRow) -> Option<Self> {
            Self::from_postgres_row(row)
        }

        fn from_postgres_row(row: &dinoco_engine::PostgresRow) -> Option<Self> {
            Some(Self {
                item: CS::from_postgres_row(row)?,
                ordinal: postgres_relation_ordinal(row, CS::FIELDS.len() + 1)?,
                marker: PhantomData,
            })
        }
    }

    /// The parent ordinal column: `INT4` for a `VALUES` list, `INT8` otherwise.
    fn postgres_relation_ordinal(row: &dinoco_engine::PostgresRow, index: usize) -> Option<usize> {
        if *row.columns().get(index)?.type_() == dinoco_engine::tokio_postgres::types::Type::INT4 {
            row.try_get::<_, i32>(index).ok()?.try_into().ok()
        } else {
            row.try_get::<_, i64>(index).ok()?.try_into().ok()
        }
    }
}

dinoco_engine::__dinoco_mysql! {
    impl<C, CS> dinoco_engine::DinocoMysql for RelationOccurrenceRow<C, CS>
    where
        C: DinocoEntity,
        CS: DinocoProjection<C>,
    {
        fn from_mysql_row(row: &dinoco_engine::MysqlRow) -> Option<Self> {
            Self::from_mysql_row_owned(row.clone())
        }

        fn from_mysql_row_owned(mut row: dinoco_engine::MysqlRow) -> Option<Self> {
            let ordinal = dinoco_engine::mysql_take::<i64>(&mut row, CS::FIELDS.len() + 1)?.try_into().ok()?;

            Some(Self { item: CS::from_mysql_row_owned(row)?, ordinal, marker: PhantomData })
        }
    }
}

/// The relation keys of `parents`, and which parents have one: the loaded
/// rows are numbered by the position of their parent's key.
fn relation_keys<S>(parents: &[S], field: &'static str) -> (Vec<DinocoValue>, Vec<bool>)
where
    S: DinocoRelationValue,
{
    let mut keys = Vec::with_capacity(parents.len());
    let has_key = parents
        .iter()
        .map(|parent| match parent.dinoco_relation_value(field) {
            Some(key) => {
                keys.push(key);
                true
            }
            None => false,
        })
        .collect();

    (keys, has_key)
}

fn noop_include_applier<S>() -> IncludeApplier<S> {
    Box::new(|_| {})
}
