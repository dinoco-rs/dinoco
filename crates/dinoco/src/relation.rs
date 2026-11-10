use std::any::{Any, type_name};
use std::future::Future;
use std::marker::PhantomData;
use std::pin::Pin;

use dinoco_engine::{
    AggregateColumn, AggregateFunction, AggregateGroup, AggregateList, AggregateQuery, DinocoEntity, DinocoProjection,
    DinocoRowModel, DinocoValue, FindOrderBy, FindQuery, FindWhere, GroupBy, ManyToManyRelationQuery, PluckColumn,
    PluckColumns, RelationAggregateQuery, RelationJoinTable, RelationOccurrenceQuery, WhereComplex,
};
use futures_util::future::Either;

use crate::{Aggregate, AppendValue, IntoGroupKey, ReadTarget, aggregate_group};

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

/// Not an `async fn`, which would keep its arguments twice; and without
/// includes, as most reads are, there is nothing to wait for.
pub(crate) fn load_includes<'a, S>(
    includes: Vec<Box<dyn IncludeLoader<S>>>,
    target: &'a ReadTarget<'_>,
    items: &'a mut [S],
) -> impl Future<Output = anyhow::Result<()>> + 'a
where
    S: 'a,
{
    if includes.is_empty() {
        return Either::Left(std::future::ready(Ok(())));
    }

    Either::Right(async move {
        let appliers =
            futures_util::future::try_join_all(includes.iter().map(|include| include.load_applier(target, items)))
                .await?;

        for apply in appliers {
            apply(items);
        }

        Ok(())
    })
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
        self.query.order_by.push(closure(C::OrderBy::default()));

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
        self.query.order_by.push(closure(C::OrderBy::default()));

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

/// Write access to a model's columns by name, which a grouped include uses to
/// fill the group key and the aggregates into each group's row. Generated by
/// `#[derive(Entity)]` and `#[derive(EntityExtend)]`.
#[doc(hidden)]
pub trait DinocoFieldSlot {
    fn dinoco_field_slot(&mut self, field: &str) -> Option<&mut dyn Any>;
}

impl<M, C, CS> HasMany<M, C, CS>
where
    C: DinocoEntity,
{
    /// Loads aggregates of the related rows instead of the rows: once per
    /// parent, or once per group after [`group_by`](RelationAggregate::group_by).
    ///
    /// The relation field keeps its type: it is filled with one row per
    /// group, whose grouped field holds the group key and whose aggregated
    /// fields hold the aggregates; every other field keeps its default. After
    /// `.select::<P>()` each group is a `P` instead, a projection holding only
    /// the fields filled (it needs `#[derive(Default)]`), and the parent's
    /// relation field is a `Vec<P>`.
    ///
    /// Chain `.pluck(...)` to compute several aggregates; each must be of a
    /// different field, and fit it: a `sum`, `min` or `max` fits the field it
    /// reads, an `avg` only a `Float` field, a `count` only an `Integer` one.
    /// Grouping a `DateTime` field by `date()`, `week()`, `month()` or
    /// `year()` stores the bucket's first day at midnight UTC.
    ///
    /// `where_` and `where_complex` pick the related rows aggregated. After
    /// `group_by`, `order_by` on the grouped or an aggregated field orders
    /// each parent's groups (by key, ascending, by default), and `take` and
    /// `skip` page them. Includes nested in this one are not loaded.
    ///
    /// ```ignore
    /// let accounts = find_many::<Account>()
    ///     .includes(|x| {
    ///         x.transactions()
    ///             .pluck(|t| t.amount.sum())
    ///             .pluck(|t| t.fee.sum())
    ///             .group_by(|t| t.kind)
    ///     })
    ///     .execute(&client)
    ///     .await?;
    /// // accounts[0].transactions: one `Transaction` per kind, with its
    /// // `amount` and `fee` sums.
    /// ```
    pub fn pluck<F, V>(self, callback: F) -> RelationAggregate<M, C, CS, V>
    where
        F: FnOnce(C::Where) -> Aggregate<V>,
        V: PluckColumn,
    {
        let aggregate = callback(C::Where::default()).column();

        RelationAggregate {
            parts: AggregateParts {
                relation: self.relation,
                parent_field: self.parent_field,
                child_field: self.child_field,
                join: self.many_to_many.map(|join| RelationJoinTable {
                    table: join.join_table,
                    parent_field: join.join_parent_field,
                    child_field: join.join_child_field,
                }),
                query: self.query,
                aggregates: AggregateList::from(aggregate),
            },
            marker: PhantomData,
        }
    }
}

/// What every aggregate include holds, apart from its types.
struct AggregateParts {
    relation: &'static str,
    parent_field: &'static str,
    child_field: &'static str,
    join: Option<RelationJoinTable>,
    query: FindQuery,
    aggregates: AggregateList,
}

impl AggregateParts {
    fn push<V>(mut self, aggregate: Aggregate<V>) -> Self {
        self.aggregates.push(aggregate.column());
        self
    }

    fn group(&self, key: GroupBy) -> (AggregateGroup, Option<&'static str>) {
        aggregate_group(key, &self.aggregates, self.query.order_by.as_slice(), self.query.limit, self.query.skip)
    }

    fn include(self, group: Option<AggregateGroup>, unordered_field: Option<&'static str>) -> AggregateInclude {
        let FindQuery { from, conditions, .. } = self.query;
        let mut source = FindQuery::new(&[], from, -1, -1);
        source.conditions = conditions;

        AggregateInclude {
            relation: self.relation,
            parent_field: self.parent_field,
            query: RelationAggregateQuery {
                query: AggregateQuery { source, aggregates: self.aggregates, group },
                child_field: self.child_field,
                join: self.join,
                key_count: 0,
            },
            unordered_field,
        }
    }
}

/// An include that loads one aggregate per parent over its related rows, from
/// `.includes(|x| x.relation().pluck(...))`. See [`HasMany::pluck`].
pub struct RelationAggregate<M, C, CS, V> {
    parts: AggregateParts,
    marker: PhantomData<fn() -> (M, C, CS, V)>,
}

impl<M, C, CS, V> RelationAggregate<M, C, CS, V>
where
    C: DinocoEntity,
{
    /// Computes another aggregate, of another field, stored in that field.
    pub fn pluck<F, U>(self, callback: F) -> RelationAggregates<M, C, CS, (V, U)>
    where
        F: FnOnce(C::Where) -> Aggregate<U>,
        U: PluckColumn,
    {
        RelationAggregates { parts: self.parts.push(callback(C::Where::default())), marker: PhantomData }
    }

    /// Computes the aggregate once per group of each parent's related rows
    /// sharing a key, loaded as one row per group. See [`HasMany::pluck`].
    pub fn group_by<F, G>(self, callback: F) -> RelationGroup<M, C, CS, G::Key, V>
    where
        F: FnOnce(C::Where) -> G,
        G: IntoGroupKey,
    {
        let (group, unordered_field) = self.parts.group(callback(C::Where::default()).into_group_by());

        RelationGroup { parts: self.parts, group, unordered_field, marker: PhantomData }
    }
}

/// An include that loads several aggregates per parent, from chained
/// `.pluck(...)` calls. See [`HasMany::pluck`].
pub struct RelationAggregates<M, C, CS, T> {
    parts: AggregateParts,
    marker: PhantomData<fn() -> (M, C, CS, T)>,
}

impl<M, C, CS, T> RelationAggregates<M, C, CS, T>
where
    C: DinocoEntity,
    T: PluckColumns,
{
    /// Computes one more aggregate, of another field, stored in that field.
    pub fn pluck<F, U>(self, callback: F) -> RelationAggregates<M, C, CS, T::Output>
    where
        F: FnOnce(C::Where) -> Aggregate<U>,
        U: PluckColumn,
        T: AppendValue<U>,
    {
        RelationAggregates { parts: self.parts.push(callback(C::Where::default())), marker: PhantomData }
    }

    /// Computes every aggregate once per group of each parent's related rows
    /// sharing a key, loaded as one row per group. See [`HasMany::pluck`].
    pub fn group_by<F, G>(self, callback: F) -> RelationGroups<M, C, CS, G::Key, T>
    where
        F: FnOnce(C::Where) -> G,
        G: IntoGroupKey,
    {
        let (group, unordered_field) = self.parts.group(callback(C::Where::default()).into_group_by());

        RelationGroups { parts: self.parts, group, unordered_field, marker: PhantomData }
    }
}

/// An include that loads one aggregate per group of each parent's related
/// rows, from `.includes(|x| x.relation().pluck(...).group_by(...))`. See
/// [`HasMany::pluck`].
pub struct RelationGroup<M, C, CS, K, V> {
    parts: AggregateParts,
    group: AggregateGroup,
    unordered_field: Option<&'static str>,
    marker: PhantomData<fn() -> (M, C, CS, K, V)>,
}

/// An include that loads several aggregates per group of each parent's
/// related rows, from chained `.pluck(...)` calls and `.group_by(...)`. See
/// [`HasMany::pluck`].
pub struct RelationGroups<M, C, CS, K, T> {
    parts: AggregateParts,
    group: AggregateGroup,
    unordered_field: Option<&'static str>,
    marker: PhantomData<fn() -> (M, C, CS, K, T)>,
}

/// What an aggregate include runs, apart from its types.
struct AggregateInclude {
    relation: &'static str,
    parent_field: &'static str,
    query: RelationAggregateQuery,
    /// A field `order_by` named that is neither the key nor an aggregated
    /// one: the groups have no such column to be ordered by.
    unordered_field: Option<&'static str>,
}

/// The loader of an aggregate include of `C`'s rows, loaded as `CS` and
/// decoded as `R`.
struct AggregateLoader<C, CS, R> {
    include: AggregateInclude,
    marker: PhantomData<fn() -> (C, CS, R)>,
}

macro_rules! aggregate_include_loaders {
    ($($builder:ident<$($param:ident),+> => $row:ty, $grouped:expr;)*) => {
        $(
            impl<M, S, C, CS, $($param),+> IntoIncludeLoader<M, S> for $builder<M, C, CS, $($param),+>
            where
                M: DinocoEntity + 'static,
                S: DinocoRelationValue + DinocoRelationApply<CS> + Sync + 'static,
                C: DinocoEntity + 'static,
                CS: DinocoFieldSlot + Default + Send + 'static,
                $row: AggregateIncludeRow,
            {
                fn into_include_loader(self) -> Box<dyn IncludeLoader<S>> {
                    #[allow(clippy::redundant_closure_call)]
                    let include = ($grouped)(self);

                    Box::new(AggregateLoader::<C, CS, $row> { include, marker: PhantomData })
                }
            }
        )*
    };
}

aggregate_include_loaders! {
    RelationAggregate<V> => AggregateRow<V>, |this: RelationAggregate<M, C, CS, V>| this.parts.include(None, None);
    RelationAggregates<T> => AggregatesRow<T>, |this: RelationAggregates<M, C, CS, T>| this.parts.include(None, None);
    RelationGroup<K, V> => GroupRow<K, V>, |this: RelationGroup<M, C, CS, K, V>| {
        this.parts.include(Some(this.group), this.unordered_field)
    };
    RelationGroups<K, T> => GroupsRow<K, T>, |this: RelationGroups<M, C, CS, K, T>| {
        this.parts.include(Some(this.group), this.unordered_field)
    };
}

/// A row of an aggregate include: the parent it belongs to, and how it fills
/// the related row.
trait AggregateIncludeRow: DinocoRowModel {
    fn ordinal(&self) -> usize;

    /// Whether the key and every aggregate fit their fields of `model`, a
    /// row of `C`'s table; otherwise why not.
    fn check<C: DinocoEntity, CS: DinocoFieldSlot>(
        model: &mut CS,
        key: Option<&'static str>,
        aggregates: &[AggregateColumn],
    ) -> Result<(), String>;

    fn store<CS: DinocoFieldSlot>(self, model: &mut CS, key: Option<&'static str>, aggregates: &[AggregateColumn]);
}

/// Whether a value of type `type_name` that `fits` can be stored in the field
/// `field` of `model`, a row of `C`'s table.
fn check_slot<C, CS>(
    model: &mut CS,
    field: &'static str,
    what: &str,
    type_name: &str,
    fits: fn(&dyn Any) -> bool,
) -> Result<(), String>
where
    C: DinocoEntity,
    CS: DinocoFieldSlot,
{
    match model.dinoco_field_slot(field) {
        Some(slot) if fits(slot) => Ok(()),
        Some(_) => Err(format!(
            "{what} (a `{type_name}`) can't be stored in `{}.{field}`: a grouped include writes each group into a \
             row of the related model, so every value must fit its field's type",
            C::TABLE_NAME
        )),
        None => Err(format!("`{}` has no field `{field}` to store {what} in", short_type_name::<CS>())),
    }
}

/// The name of `T` without its module path.
fn short_type_name<T>() -> &'static str {
    let name = type_name::<T>();
    name.rsplit("::").next().unwrap_or(name)
}

fn aggregate_name(aggregate: AggregateColumn) -> String {
    let function = match aggregate.function {
        AggregateFunction::SumInteger | AggregateFunction::SumFloat => "sum",
        AggregateFunction::Avg => "avg",
        AggregateFunction::Min => "min",
        AggregateFunction::Max => "max",
        AggregateFunction::Count => "count",
    };
    format!("the {function} of `{}`", aggregate.field)
}

/// Checks the key and that no two aggregates share a field, which holds one.
fn check_key_and_fields<C, CS, K>(
    model: &mut CS,
    key: Option<&'static str>,
    aggregates: &[AggregateColumn],
) -> Result<(), String>
where
    C: DinocoEntity,
    CS: DinocoFieldSlot,
    K: PluckColumn,
{
    for (index, aggregate) in aggregates.iter().enumerate() {
        if let Some(earlier) = aggregates[..index].iter().find(|earlier| earlier.field == aggregate.field) {
            return Err(format!(
                "{} and {} can't both be stored in `{}.{}`: a grouped include stores each aggregate in the field it \
                 reads, so pluck one aggregate per field",
                aggregate_name(*earlier),
                aggregate_name(*aggregate),
                C::TABLE_NAME,
                aggregate.field
            ));
        }
    }
    if let Some(key) = key {
        check_slot::<C, CS>(model, key, &format!("the group key of `{key}`"), type_name::<K>(), K::dinoco_fits)?;
    }
    Ok(())
}

/// Stores `value` in the field `field` of `model`, if it has one.
fn store_value<CS: DinocoFieldSlot, V: PluckColumn>(model: &mut CS, field: &str, value: V) {
    if let Some(slot) = model.dinoco_field_slot(field) {
        value.dinoco_store(slot);
    }
}

/// Stores each of `values` in the field its aggregate reads.
fn store_values<CS: DinocoFieldSlot, T: PluckColumns>(model: &mut CS, aggregates: &[AggregateColumn], values: T) {
    values.dinoco_store_each(&mut |position, store| {
        aggregates
            .get(position)
            .and_then(|aggregate| model.dinoco_field_slot(aggregate.field))
            .is_some_and(|slot| store(slot))
    });
}

/// One aggregate per parent: the value, then the parent ordinal.
struct AggregateRow<V> {
    value: V,
    ordinal: usize,
}

impl<V: PluckColumn> AggregateIncludeRow for AggregateRow<V> {
    fn ordinal(&self) -> usize {
        self.ordinal
    }

    fn check<C: DinocoEntity, CS: DinocoFieldSlot>(
        model: &mut CS,
        _key: Option<&'static str>,
        aggregates: &[AggregateColumn],
    ) -> Result<(), String> {
        let aggregate = aggregates[0];
        check_slot::<C, CS>(model, aggregate.field, &aggregate_name(aggregate), type_name::<V>(), V::dinoco_fits)
    }

    fn store<CS: DinocoFieldSlot>(self, model: &mut CS, _key: Option<&'static str>, aggregates: &[AggregateColumn]) {
        store_value(model, aggregates[0].field, self.value);
    }
}

/// Several aggregates per parent: the values, then the parent ordinal.
struct AggregatesRow<T> {
    values: T,
    ordinal: usize,
}

impl<T: PluckColumns> AggregateIncludeRow for AggregatesRow<T> {
    fn ordinal(&self) -> usize {
        self.ordinal
    }

    fn check<C: DinocoEntity, CS: DinocoFieldSlot>(
        model: &mut CS,
        _key: Option<&'static str>,
        aggregates: &[AggregateColumn],
    ) -> Result<(), String> {
        check_key_and_fields::<C, CS, bool>(model, None, aggregates)?;
        check_values::<C, CS, T>(model, aggregates)
    }

    fn store<CS: DinocoFieldSlot>(self, model: &mut CS, _key: Option<&'static str>, aggregates: &[AggregateColumn]) {
        store_values(model, aggregates, self.values);
    }
}

/// Whether each of the values of `T` fits the field its aggregate reads.
fn check_values<C, CS, T>(model: &mut CS, aggregates: &[AggregateColumn]) -> Result<(), String>
where
    C: DinocoEntity,
    CS: DinocoFieldSlot,
    T: PluckColumns,
{
    for (position, aggregate) in aggregates.iter().enumerate() {
        let fits = model.dinoco_field_slot(aggregate.field).is_some_and(|slot| T::dinoco_fits_at(position, slot));
        if !fits {
            // Not stored: says whether the field is missing or of another type.
            let type_name = T::dinoco_type_name_at(position);
            check_slot::<C, CS>(model, aggregate.field, &aggregate_name(*aggregate), type_name, |_| false)?;
        }
    }
    Ok(())
}

/// One aggregate per group: the key, the value, then the parent ordinal.
struct GroupRow<K, V> {
    key: K,
    value: V,
    ordinal: usize,
}

impl<K: PluckColumn, V: PluckColumn> AggregateIncludeRow for GroupRow<K, V> {
    fn ordinal(&self) -> usize {
        self.ordinal
    }

    fn check<C: DinocoEntity, CS: DinocoFieldSlot>(
        model: &mut CS,
        key: Option<&'static str>,
        aggregates: &[AggregateColumn],
    ) -> Result<(), String> {
        check_key_and_fields::<C, CS, K>(model, key, aggregates)?;
        let aggregate = aggregates[0];
        check_slot::<C, CS>(model, aggregate.field, &aggregate_name(aggregate), type_name::<V>(), V::dinoco_fits)
    }

    fn store<CS: DinocoFieldSlot>(self, model: &mut CS, key: Option<&'static str>, aggregates: &[AggregateColumn]) {
        store_value(model, aggregates[0].field, self.value);
        // The key last: grouping and aggregating one field keeps the key.
        if let Some(key) = key {
            store_value(model, key, self.key);
        }
    }
}

/// Several aggregates per group: the key, the values, then the parent ordinal.
struct GroupsRow<K, T> {
    key: K,
    values: T,
    ordinal: usize,
}

impl<K: PluckColumn, T: PluckColumns> AggregateIncludeRow for GroupsRow<K, T> {
    fn ordinal(&self) -> usize {
        self.ordinal
    }

    fn check<C: DinocoEntity, CS: DinocoFieldSlot>(
        model: &mut CS,
        key: Option<&'static str>,
        aggregates: &[AggregateColumn],
    ) -> Result<(), String> {
        check_key_and_fields::<C, CS, K>(model, key, aggregates)?;
        check_values::<C, CS, T>(model, aggregates)
    }

    fn store<CS: DinocoFieldSlot>(self, model: &mut CS, key: Option<&'static str>, aggregates: &[AggregateColumn]) {
        store_values(model, aggregates, self.values);
        if let Some(key) = key {
            store_value(model, key, self.key);
        }
    }
}

impl<S, C, CS, R> IncludeLoader<S> for AggregateLoader<C, CS, R>
where
    S: DinocoRelationValue + DinocoRelationApply<CS> + Sync + 'static,
    C: DinocoEntity + 'static,
    CS: DinocoFieldSlot + Default + Send + 'static,
    R: AggregateIncludeRow,
{
    fn load_applier<'a>(&'a self, target: &'a ReadTarget<'a>, parents: &'a [S]) -> IncludeLoaderFuture<'a, S> {
        Box::pin(async move {
            let include = &self.include;
            let table = include.query.query.source.from;
            if let Some(field) = include.unordered_field {
                anyhow::bail!(
                    "a grouped include of `{table}` can only be ordered by the grouped or an aggregated field, not \
                     by `{field}`"
                );
            }

            let key_field = include.query.query.group.map(|group| group.key.field());
            let aggregates = include.query.query.aggregates.as_slice();
            R::check::<C, CS>(&mut CS::default(), key_field, aggregates).map_err(anyhow::Error::msg)?;

            let (keys, has_key) = relation_keys(parents, include.parent_field);
            if keys.is_empty() {
                return Ok(noop_include_applier());
            }

            let key_count = keys.len();
            let mut query = include.query.clone();
            query.key_count = key_count;
            let rows = target.query_relation_aggregate::<R>(query, keys).await?;

            let mut counts = vec![0usize; key_count];
            for row in &rows {
                if let Some(count) = counts.get_mut(row.ordinal()) {
                    *count += 1;
                }
            }
            let mut grouped = counts.into_iter().map(Vec::with_capacity).collect::<Vec<Vec<CS>>>();
            for row in rows {
                if let Some(values) = grouped.get_mut(row.ordinal()) {
                    let mut model = CS::default();
                    row.store(&mut model, key_field, aggregates);
                    values.push(model);
                }
            }

            let relation = include.relation;
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

dinoco_engine::__dinoco_sqlite! {
    impl<V: PluckColumn> dinoco_engine::DinocoSqlite for AggregateRow<V> {
        fn from_sqlite_row(row: &dinoco_engine::SqliteRow<'_>) -> Option<Self> {
            Some(Self { value: V::from_sqlite_column(row, 0)?, ordinal: sqlite_ordinal(row, 1)? })
        }
    }

    impl<T: PluckColumns> dinoco_engine::DinocoSqlite for AggregatesRow<T> {
        fn from_sqlite_row(row: &dinoco_engine::SqliteRow<'_>) -> Option<Self> {
            Some(Self { values: T::from_sqlite_columns(row, 0)?, ordinal: sqlite_ordinal(row, T::LEN)? })
        }
    }

    impl<K: PluckColumn, V: PluckColumn> dinoco_engine::DinocoSqlite for GroupRow<K, V> {
        fn from_sqlite_row(row: &dinoco_engine::SqliteRow<'_>) -> Option<Self> {
            Some(Self {
                key: K::from_sqlite_column(row, 0)?,
                value: V::from_sqlite_column(row, 1)?,
                ordinal: sqlite_ordinal(row, 2)?,
            })
        }
    }

    impl<K: PluckColumn, T: PluckColumns> dinoco_engine::DinocoSqlite for GroupsRow<K, T> {
        fn from_sqlite_row(row: &dinoco_engine::SqliteRow<'_>) -> Option<Self> {
            Some(Self {
                key: K::from_sqlite_column(row, 0)?,
                values: T::from_sqlite_columns(row, 1)?,
                ordinal: sqlite_ordinal(row, 1 + T::LEN)?,
            })
        }
    }

    fn sqlite_ordinal(row: &dinoco_engine::SqliteRow<'_>, index: usize) -> Option<usize> {
        row.get::<_, i64>(index).ok()?.try_into().ok()
    }
}

dinoco_engine::__dinoco_postgres! {
    impl<V: PluckColumn> dinoco_engine::DinocoPostgres for AggregateRow<V> {
        fn from_deadpool_posgres_row(row: &dinoco_engine::DeadpoolPostgresRow) -> Option<Self> {
            Self::from_postgres_row(row)
        }

        fn from_postgres_row(row: &dinoco_engine::PostgresRow) -> Option<Self> {
            Some(Self { value: V::from_postgres_column(row, 0)?, ordinal: postgres_relation_ordinal(row, 1)? })
        }
    }

    impl<T: PluckColumns> dinoco_engine::DinocoPostgres for AggregatesRow<T> {
        fn from_deadpool_posgres_row(row: &dinoco_engine::DeadpoolPostgresRow) -> Option<Self> {
            Self::from_postgres_row(row)
        }

        fn from_postgres_row(row: &dinoco_engine::PostgresRow) -> Option<Self> {
            Some(Self { values: T::from_postgres_columns(row, 0)?, ordinal: postgres_relation_ordinal(row, T::LEN)? })
        }
    }

    impl<K: PluckColumn, V: PluckColumn> dinoco_engine::DinocoPostgres for GroupRow<K, V> {
        fn from_deadpool_posgres_row(row: &dinoco_engine::DeadpoolPostgresRow) -> Option<Self> {
            Self::from_postgres_row(row)
        }

        fn from_postgres_row(row: &dinoco_engine::PostgresRow) -> Option<Self> {
            Some(Self {
                key: K::from_postgres_column(row, 0)?,
                value: V::from_postgres_column(row, 1)?,
                ordinal: postgres_relation_ordinal(row, 2)?,
            })
        }
    }

    impl<K: PluckColumn, T: PluckColumns> dinoco_engine::DinocoPostgres for GroupsRow<K, T> {
        fn from_deadpool_posgres_row(row: &dinoco_engine::DeadpoolPostgresRow) -> Option<Self> {
            Self::from_postgres_row(row)
        }

        fn from_postgres_row(row: &dinoco_engine::PostgresRow) -> Option<Self> {
            Some(Self {
                key: K::from_postgres_column(row, 0)?,
                values: T::from_postgres_columns(row, 1)?,
                ordinal: postgres_relation_ordinal(row, 1 + T::LEN)?,
            })
        }
    }
}

dinoco_engine::__dinoco_mysql! {
    impl<V: PluckColumn> dinoco_engine::DinocoMysql for AggregateRow<V> {
        fn from_mysql_row(row: &dinoco_engine::MysqlRow) -> Option<Self> {
            Self::from_mysql_row_owned(row.clone())
        }

        fn from_mysql_row_owned(mut row: dinoco_engine::MysqlRow) -> Option<Self> {
            let ordinal = dinoco_engine::mysql_take::<i64>(&mut row, 1)?.try_into().ok()?;

            Some(Self { value: V::from_mysql_column(&mut row, 0)?, ordinal })
        }
    }

    impl<T: PluckColumns> dinoco_engine::DinocoMysql for AggregatesRow<T> {
        fn from_mysql_row(row: &dinoco_engine::MysqlRow) -> Option<Self> {
            Self::from_mysql_row_owned(row.clone())
        }

        fn from_mysql_row_owned(mut row: dinoco_engine::MysqlRow) -> Option<Self> {
            let ordinal = dinoco_engine::mysql_take::<i64>(&mut row, T::LEN)?.try_into().ok()?;

            Some(Self { values: T::from_mysql_columns(&mut row, 0)?, ordinal })
        }
    }

    impl<K: PluckColumn, V: PluckColumn> dinoco_engine::DinocoMysql for GroupRow<K, V> {
        fn from_mysql_row(row: &dinoco_engine::MysqlRow) -> Option<Self> {
            Self::from_mysql_row_owned(row.clone())
        }

        fn from_mysql_row_owned(mut row: dinoco_engine::MysqlRow) -> Option<Self> {
            let ordinal = dinoco_engine::mysql_take::<i64>(&mut row, 2)?.try_into().ok()?;

            Some(Self { key: K::from_mysql_column(&mut row, 0)?, value: V::from_mysql_column(&mut row, 1)?, ordinal })
        }
    }

    impl<K: PluckColumn, T: PluckColumns> dinoco_engine::DinocoMysql for GroupsRow<K, T> {
        fn from_mysql_row(row: &dinoco_engine::MysqlRow) -> Option<Self> {
            Self::from_mysql_row_owned(row.clone())
        }

        fn from_mysql_row_owned(mut row: dinoco_engine::MysqlRow) -> Option<Self> {
            let ordinal = dinoco_engine::mysql_take::<i64>(&mut row, 1 + T::LEN)?.try_into().ok()?;

            Some(Self { key: K::from_mysql_column(&mut row, 0)?, values: T::from_mysql_columns(&mut row, 1)?, ordinal })
        }
    }
}
