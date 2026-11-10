use std::future::Future;
use std::marker::PhantomData;

use dinoco_engine::{
    AggregateColumn, AggregateGroup, AggregateList, AggregateQuery, CountQuery, DinocoEntity, DinocoProjection,
    DinocoRowModel, FindOrderBy, FindQuery, FindWhere, GroupBy, GroupOrder, GroupedValue, GroupedValues, PluckColumn,
    PluckColumns, PluckValue, PluckValues, WhereComplex,
};
use futures_util::TryFutureExt;
use futures_util::future::Either;

use crate::{
    Aggregate, Count, DinocoCountModel, Field, IncludeLoader, IntoCursor, IntoGroupKey, IntoIncludeLoader,
    IntoSubquery, KeysetCursor, ReadExecutor, apply_cursor_to_find, load_includes, single_field,
};

/// `C` is [`NoCount`] until `.count()` or one of its variants turns it into a
/// [`WithCount`], whose `execute` also returns an `M::Count`.
pub struct FindMany<M, S = M, C = NoCount> {
    query: FindQuery,
    includes: Vec<Box<dyn IncludeLoader<S>>>,
    read_primary: bool,
    complex_where: bool,
    count: C,
    /// `.after(...)`/`.before(...)`, applied when the query runs.
    cursor: Option<Box<KeysetCursor>>,

    select_marker: PhantomData<S>,
    marker: PhantomData<M>,
}

/// A `find_many` that returns its rows alone, the default.
pub struct NoCount;

/// A `find_many` that also counts, from `.count()`, `.count_transform(...)`,
/// `.count_custom(...)` or `.count_custom_complex(...)`.
pub struct WithCount<F>(F);

/// What a counting `find_many` counts with: [`SameFilters`],
/// [`TransformedFilters`] or [`CustomFilters`].
pub trait CountFilters<M>
where
    M: DinocoEntity,
{
    /// Whether the count runs with the find's own filters, so that a total at
    /// or below `skip` leaves the find no row to return.
    #[doc(hidden)]
    const FIND_FILTERS: bool = false;

    /// The count, given the find's conditions.
    #[doc(hidden)]
    fn into_count(self, find_conditions: &[FindWhere]) -> Count<M>;
}

/// `.count()`: the find's own filters.
pub struct SameFilters;

/// `.count_transform(...)`: the find's filters, changed for the count.
pub struct TransformedFilters<F>(F);

/// `.count_custom(...)` and `.count_custom_complex(...)`: filters of the
/// count's own.
pub struct CustomFilters<F>(F);

impl<M> CountFilters<M> for SameFilters
where
    M: DinocoEntity,
{
    const FIND_FILTERS: bool = true;

    fn into_count(self, find_conditions: &[FindWhere]) -> Count<M> {
        Count::with_conditions(find_conditions.to_vec())
    }
}

impl<M, F> CountFilters<M> for TransformedFilters<F>
where
    M: DinocoEntity,
    F: FnOnce(Count<M>) -> Count<M>,
{
    fn into_count(self, find_conditions: &[FindWhere]) -> Count<M> {
        (self.0)(Count::with_conditions(find_conditions.to_vec()))
    }
}

impl<M, F> CountFilters<M> for CustomFilters<F>
where
    M: DinocoEntity,
    F: FnOnce(Count<M>) -> Count<M>,
{
    fn into_count(self, _find_conditions: &[FindWhere]) -> Count<M> {
        (self.0)(Count::with_conditions(Vec::new()))
    }
}

impl<M, S, C> FindMany<M, S, C>
where
    M: DinocoEntity + DinocoRowModel,
    S: DinocoRowModel,
{
    pub fn where_<F>(mut self, callback: F) -> Self
    where
        F: FnOnce(M::Where) -> FindWhere,
    {
        if !self.complex_where {
            self.query.conditions.push(callback(M::Where::default()));
        }

        self
    }

    pub fn where_complex<F>(mut self, callback: F) -> Self
    where
        F: FnOnce(M::Where, WhereComplex) -> FindWhere,
    {
        self.query.conditions = vec![callback(M::Where::default(), WhereComplex)];
        self.complex_where = true;

        self
    }

    pub fn select<NS>(mut self) -> FindMany<M, NS, C>
    where
        NS: DinocoProjection<M>,
    {
        self.query.fields = NS::FIELDS;

        FindMany {
            query: self.query,
            includes: Vec::new(),
            read_primary: self.read_primary,
            complex_where: self.complex_where,
            count: self.count,
            cursor: self.cursor,
            select_marker: PhantomData,
            marker: PhantomData,
        }
    }

    /// Orders the rows by a column. Each call adds a column after the ones
    /// before it, so `.order_by(a).order_by(b)` orders by `a`, then by `b`.
    pub fn order_by<F>(mut self, closure: F) -> Self
    where
        F: FnOnce(M::OrderBy) -> FindOrderBy,
    {
        self.query.order_by.push(closure(M::OrderBy::default()));

        self
    }

    pub fn includes<F, I>(mut self, closure: F) -> Self
    where
        F: FnOnce(M::Include) -> I,
        I: IntoIncludeLoader<M, S>,
    {
        self.includes.push(closure(M::Include::default()).into_include_loader());

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

    /// Keyset pagination: keeps only the rows that come after `cursor` in the
    /// `order_by` order. `cursor` is the last row of the previous page (any
    /// model or `select` projection holding the `order_by` columns), a
    /// [`Cursor`](crate::Cursor), or a tuple with one value per `order_by`
    /// column, in order.
    ///
    /// Unlike `skip`, the page does not shift when rows are inserted or
    /// deleted before it, and the database seeks to it through an index on the
    /// `order_by` columns instead of reading and discarding the rows before
    /// it. End the `order_by` with a unique column (such as `id`) so no two
    /// rows share a position. A row cursor reads the `order_by` columns known
    /// when `after` is called, so call `order_by` first.
    ///
    /// `take` caps the page and `skip` still skips rows after the cursor.
    /// The cursor is not counted by `.count()`, which keeps reporting the
    /// total of the whole list. Cursor values can't be `NULL`.
    pub fn after<K>(mut self, cursor: K) -> Self
    where
        K: IntoCursor,
    {
        self.cursor = Some(KeysetCursor::new(cursor, &self.query.order_by, false));

        self
    }

    /// Keyset pagination backwards: keeps only the rows that come before
    /// `cursor` in the `order_by` order, the previous page. With `take(n)` it
    /// returns the `n` rows right before the cursor, still in `order_by`
    /// order. Takes the same cursors as [`after`](Self::after).
    pub fn before<K>(mut self, cursor: K) -> Self
    where
        K: IntoCursor,
    {
        self.cursor = Some(KeysetCursor::new(cursor, &self.query.order_by, true));

        self
    }

    pub fn read_in_primary(mut self) -> Self {
        self.read_primary = true;

        self
    }

    /// Also counts the rows the find's filters match, ignoring `order_by`,
    /// `take` and `skip`: the total a paginated list shows. `execute` then
    /// returns `(rows, M::Count)`.
    ///
    /// The filters are read when the query runs, so `where_` calls after this
    /// one count too. Of `count` and its variants, the last one called is the
    /// one that runs.
    pub fn count(self) -> FindMany<M, S, WithCount<SameFilters>> {
        self.counting(SameFilters)
    }

    /// [`count`](Self::count) with the find's filters changed for the count
    /// only. `callback` receives a [`count`](crate::count) builder holding
    /// them as `where_` calls, so it can [`remove`](Count::remove) some, add
    /// `where_` and `where_complex` filters, and ask for relation counts with
    /// `includes`. The find keeps its own filters.
    pub fn count_transform<F>(self, callback: F) -> FindMany<M, S, WithCount<TransformedFilters<F>>>
    where
        F: FnOnce(Count<M>) -> Count<M>,
    {
        self.counting(TransformedFilters(callback))
    }

    /// [`count`](Self::count) with filters of its own instead of the find's:
    /// `callback` receives an empty [`count`](crate::count) builder.
    pub fn count_custom<F>(self, callback: F) -> FindMany<M, S, WithCount<CustomFilters<F>>>
    where
        F: FnOnce(Count<M>) -> Count<M>,
    {
        self.counting(CustomFilters(callback))
    }

    /// [`count_custom`](Self::count_custom) with a [`WhereComplex`] tree, as
    /// [`where_complex`](Self::where_complex) takes it.
    pub fn count_custom_complex<F>(
        self,
        callback: F,
    ) -> FindMany<M, S, WithCount<CustomFilters<impl FnOnce(Count<M>) -> Count<M>>>>
    where
        F: FnOnce(M::Where, WhereComplex) -> FindWhere,
        M::Count: 'static,
    {
        self.count_custom(move |count| count.where_complex(callback))
    }

    fn counting<F>(self, filters: F) -> FindMany<M, S, WithCount<F>> {
        let Self { query, includes, read_primary, complex_where, cursor, .. } = self;

        FindMany {
            query,
            includes,
            read_primary,
            complex_where,
            count: WithCount(filters),
            cursor,
            select_marker: PhantomData,
            marker: PhantomData,
        }
    }

    /// Applies a post-query mapping to every row.
    ///
    /// Runs after includes are loaded, on the Rust side — it is not a SQL
    /// projection, so the full row (and any loaded relations) is available to
    /// the callback. After `.count()`, the count comes back unchanged next to
    /// the mapped rows.
    pub fn transform<F, R>(self, callback: F) -> FindManyTransform<M, S, F, C>
    where
        F: FnMut(S) -> R,
    {
        FindManyTransform { inner: self, transform: callback }
    }
}

impl<M, S> FindMany<M, S>
where
    M: DinocoEntity + DinocoRowModel,
    S: DinocoRowModel,
{
    /// Projects onto a single column instead of a full row, or onto an
    /// aggregate the database computes over the matching rows.
    ///
    /// With a field, this returns the raw column values directly (e.g.
    /// `Vec<i64>` for an `id` field) instead of building a row-model struct
    /// like `.select(...)`. With an aggregate (`sum`, `avg`, `min`, `max`,
    /// `count` of a field) it returns that one value, or one per group after
    /// `.group_by(...)`.
    pub fn pluck<F, P>(self, callback: F) -> P::Pluck
    where
        F: FnOnce(M::Where) -> P,
        P: IntoPluck<M>,
    {
        callback(M::Where::default()).into_pluck(self.query, self.read_primary, self.cursor)
    }

    // Not an `async fn`: the builder is taken apart first, so the future
    // keeps each part once instead of the builder and a copy of it.
    pub fn execute<C>(self, client: C) -> impl Future<Output = anyhow::Result<Vec<S>>>
    where
        C: ReadExecutor,
    {
        let Self { mut query, mut includes, read_primary, cursor, .. } = self;
        apply_cursor_to_find(cursor, &mut query, &mut includes);

        async move {
            let target = client.read_target(read_primary)?;
            let mut rows = target.query::<S>(query).await?;

            load_includes(includes, &target, &mut rows).await?;

            Ok(rows)
        }
    }
}

/// Adds the cursor of `.after(...)`/`.before(...)` to `query`. Returns
/// whether the rows read back must be reversed, for a `before` cursor.
fn apply_cursor(cursor: Option<Box<KeysetCursor>>, query: &mut FindQuery) -> anyhow::Result<bool> {
    match cursor {
        None => Ok(false),
        Some(cursor) => cursor.apply(query, true),
    }
}

impl<M, S, F> FindMany<M, S, WithCount<F>>
where
    M: DinocoEntity + DinocoRowModel,
    M::Count: DinocoCountModel<M>,
    S: DinocoRowModel,
    F: CountFilters<M>,
{
    /// Runs the count, then the find, on the same connection. With the find's
    /// own filters (`.count()`), a page that starts at or past the last row
    /// is known to be empty from the total, and its find doesn't run.
    pub fn execute<C>(self, client: C) -> impl Future<Output = anyhow::Result<(Vec<S>, M::Count)>>
    where
        C: ReadExecutor,
    {
        let Self { mut query, mut includes, read_primary, count: WithCount(filters), cursor, .. } = self;
        let has_cursor = cursor.is_some();
        apply_cursor_to_find(cursor, &mut query, &mut includes);

        async move {
            // The total counts the whole list, not what is left after the
            // cursor, whose condition is the last one.
            let find_conditions = &query.conditions[..query.conditions.len() - usize::from(has_cursor)];
            let (total, count) = count_rows::<M, F>(filters, find_conditions, &client, read_primary).await?;

            // With a cursor the find always runs: its steps (or its error)
            // come after the rows.
            if F::FIND_FILTERS && !has_cursor && total <= i64::from(query.skip.max(0)) {
                return Ok((Vec::new(), count));
            }

            let target = client.read_target(read_primary)?;
            let mut rows = target.query::<S>(query).await?;

            load_includes(includes, &target, &mut rows).await?;

            Ok((rows, count))
        }
    }
}

/// The count of a counting `find_many`: the total, then each relation count,
/// on the connection the find reads from. It runs to completion before the
/// find starts, so its state shares space with the find's in the caller's
/// future instead of adding to it.
async fn count_rows<M, F>(
    filters: F,
    find_conditions: &[FindWhere],
    client: &impl ReadExecutor,
    read_primary: bool,
) -> anyhow::Result<(i64, M::Count)>
where
    M: DinocoEntity,
    M::Count: DinocoCountModel<M>,
    F: CountFilters<M>,
{
    let (mut conditions, loaders) = filters.into_count(find_conditions).into_parts();
    // Relation counts reuse the conditions; without any, they move into the
    // query instead of being cloned.
    let query_conditions = if loaders.is_empty() { std::mem::take(&mut conditions) } else { conditions.clone() };
    let query = CountQuery { table: M::TABLE_NAME, conditions: query_conditions };

    let total = client.read_source(read_primary)?.count(query).await?;
    let mut count = M::Count::default();
    count.dinoco_set_total(total);

    // Relation counts are rare here; awaiting them boxed keeps their state
    // out of every counting find's future.
    if !loaders.is_empty() {
        let source = client.read_source(read_primary)?;
        Box::pin(async {
            for loader in loaders {
                loader.load_count(M::TABLE_NAME, "id", &conditions, &source, &mut count).await?;
            }
            anyhow::Ok(())
        })
        .await?;
    }

    Ok((total, count))
}

pub struct Pluck<M, T> {
    query: FindQuery,
    read_primary: bool,
    cursor: Option<Box<KeysetCursor>>,
    marker: PhantomData<fn() -> (M, T)>,
}

impl<M, T> Pluck<M, T>
where
    M: DinocoEntity,
    PluckValue<T>: DinocoRowModel,
{
    pub fn read_in_primary(mut self) -> Self {
        self.read_primary = true;

        self
    }

    pub fn execute<C>(self, client: C) -> impl Future<Output = anyhow::Result<Vec<T>>>
    where
        C: ReadExecutor,
    {
        let Self { mut query, read_primary, cursor, .. } = self;
        let reverse = match apply_cursor(cursor, &mut query) {
            Ok(reverse) => reverse,
            Err(error) => return Either::Left(std::future::ready(Err(error))),
        };

        Either::Right(async move {
            let target = client.read_target(read_primary)?;
            let rows = target.query::<PluckValue<T>>(query).await?;
            let mut values = rows.into_iter().map(|value| value.0).collect::<Vec<_>>();
            if reverse {
                values.reverse();
            }

            Ok(values)
        })
    }
}

/// A field pluck is a subquery for [`Field::in_query`](crate::Field::in_query).
/// Its cursor, if any, picks the rows too; an invalid one selects nothing.
impl<M, T> IntoSubquery for Pluck<M, T> {
    type Value = T;

    fn into_subquery(self) -> FindQuery {
        let Self { mut query, cursor, .. } = self;
        if let Some(cursor) = cursor
            && cursor.apply(&mut query, true).is_err()
        {
            query.conditions.push(FindWhere::Or(Vec::new()));
        }

        query
    }
}

/// What `.pluck(...)` accepts: a [`Field`], read back as `Vec<T>`, or an
/// [`Aggregate`], read back as one value (or one per group).
pub trait IntoPluck<M> {
    type Pluck;

    #[doc(hidden)]
    fn into_pluck(self, query: FindQuery, read_primary: bool, cursor: Option<Box<KeysetCursor>>) -> Self::Pluck;
}

impl<M, T, C> IntoPluck<M> for Field<T, C>
where
    M: DinocoEntity,
    PluckValue<T>: DinocoRowModel,
{
    type Pluck = Pluck<M, T>;

    fn into_pluck(self, mut query: FindQuery, read_primary: bool, cursor: Option<Box<KeysetCursor>>) -> Self::Pluck {
        query.fields = single_field(self.field_name(), M::FIELDS);

        Pluck { query, read_primary, cursor, marker: PhantomData }
    }
}

impl<M, T> IntoPluck<M> for Aggregate<T>
where
    M: DinocoEntity,
    T: PluckColumn,
{
    type Pluck = PluckAggregate<M, T>;

    fn into_pluck(self, mut query: FindQuery, read_primary: bool, cursor: Option<Box<KeysetCursor>>) -> Self::Pluck {
        query.fields = &[];

        PluckAggregate { query, aggregate: self.column(), read_primary, cursor, marker: PhantomData }
    }
}

/// An aggregate over the rows of a `find_many`, from `.pluck(...)`.
///
/// `where_` and `where_complex` pick the rows aggregated. So do `take` and
/// `skip` (with `order_by` deciding which rows they keep): the aggregate reads
/// only the rows `find_many` would return. After `.group_by(...)`, `order_by`,
/// `take` and `skip` apply to the groups instead.
///
/// A cursor (`.after(...)`/`.before(...)`) keeps only the rows past it. With
/// `take`, `before` aggregates the rows right before the cursor. After
/// `.group_by(...)` the cursor still picks the rows, read in `order_by` order
/// before it is applied to the groups.
pub struct PluckAggregate<M, T> {
    query: FindQuery,
    aggregate: AggregateColumn,
    read_primary: bool,
    cursor: Option<Box<KeysetCursor>>,
    marker: PhantomData<fn() -> (M, T)>,
}

impl<M, T> PluckAggregate<M, T>
where
    M: DinocoEntity,
    T: PluckColumn,
{
    /// Computes another aggregate in the same query, read back after this
    /// one: `.pluck(|x| x.amount.sum()).pluck(|x| x.fee.sum())` returns
    /// `(i64, i64)`. Up to eight aggregates, each of any field and function.
    pub fn pluck<F, U>(self, callback: F) -> PluckAggregates<M, (T, U)>
    where
        F: FnOnce(M::Where) -> Aggregate<U>,
        U: PluckColumn,
    {
        let mut aggregates = AggregateList::from(self.aggregate);
        aggregates.push(callback(M::Where::default()).column());

        PluckAggregates {
            query: self.query,
            aggregates,
            read_primary: self.read_primary,
            cursor: self.cursor,
            marker: PhantomData,
        }
    }

    /// Computes the aggregate once per group of rows sharing a key, read back
    /// as `Vec<(key, value)>`.
    ///
    /// The groups come ordered by key, ascending. `order_by` on the grouped
    /// field orders them by key, and on the aggregated field by value (ties
    /// by key, ascending, unless a later `order_by` on the grouped field picks
    /// the direction); `order_by` on any other field fails the query. `take`
    /// and `skip` page the groups.
    pub fn group_by<F, G>(self, callback: F) -> PluckGroup<M, G::Key, T>
    where
        F: FnOnce(M::Where) -> G,
        G: IntoGroupKey,
    {
        let key = callback(M::Where::default()).into_group_by();
        let (query, unordered_field, cursor_error) =
            grouped_query(self.query, self.cursor, AggregateList::from(self.aggregate), key);

        PluckGroup { query, unordered_field, cursor_error, read_primary: self.read_primary, marker: PhantomData }
    }

    pub fn read_in_primary(mut self) -> Self {
        self.read_primary = true;

        self
    }

    pub fn execute<C>(self, client: C) -> impl Future<Output = anyhow::Result<T>>
    where
        C: ReadExecutor,
    {
        let Self { mut query, aggregate, read_primary, cursor, .. } = self;
        if let Err(error) = apply_cursor(cursor, &mut query) {
            return Either::Left(std::future::ready(Err(error)));
        }
        let query = AggregateQuery { source: query, aggregates: AggregateList::from(aggregate), group: None };

        Either::Right(async move {
            let target = client.read_target(read_primary)?;
            let mut rows = target.aggregate::<PluckValue<T>>(query).await?;

            match rows.pop() {
                Some(PluckValue(value)) => Ok(value),
                None => anyhow::bail!("the aggregate over `{}` returned no row", M::TABLE_NAME),
            }
        })
    }
}

/// A grouped aggregate, from `.pluck(...).group_by(...)`: one `(K, T)` per
/// group of rows sharing a key.
pub struct PluckGroup<M, K, T> {
    query: AggregateQuery,
    /// A field `order_by` named that is neither the key nor the aggregated
    /// one: the groups have no such column to be ordered by.
    unordered_field: Option<&'static str>,
    /// Why the cursor of `.after(...)`/`.before(...)` could not be applied.
    cursor_error: Option<anyhow::Error>,
    read_primary: bool,
    marker: PhantomData<fn() -> (M, K, T)>,
}

impl<M, K, T> PluckGroup<M, K, T>
where
    M: DinocoEntity,
    K: PluckColumn,
    T: PluckColumn,
{
    pub fn read_in_primary(mut self) -> Self {
        self.read_primary = true;

        self
    }

    pub fn execute<C>(self, client: C) -> impl Future<Output = anyhow::Result<Vec<(K, T)>>>
    where
        C: ReadExecutor,
    {
        let Self { query, unordered_field, cursor_error, read_primary, .. } = self;

        if let Some(error) = cursor_error {
            return Either::Left(std::future::ready(Err(error)));
        }
        if let Some(field) = unordered_field {
            return Either::Left(std::future::ready(Err(anyhow::anyhow!(
                "a grouped pluck on `{}` can only be ordered by the grouped or the aggregated field, not by `{field}`",
                M::TABLE_NAME
            ))));
        }

        Either::Right(async move {
            let target = client.read_target(read_primary)?;
            let rows = target.aggregate::<GroupedValue<K, T>>(query).await?;

            Ok(rows.into_iter().map(|GroupedValue(key, value)| (key, value)).collect())
        })
    }
}

/// Several aggregates over the rows of a `find_many`, from chained
/// `.pluck(...)` calls: `T` is the tuple of their values, in order. Rows are
/// picked as for [`PluckAggregate`].
pub struct PluckAggregates<M, T> {
    query: FindQuery,
    aggregates: AggregateList,
    read_primary: bool,
    cursor: Option<Box<KeysetCursor>>,
    marker: PhantomData<fn() -> (M, T)>,
}

/// A tuple of aggregate values that takes one more: `(A, B)` becomes
/// `(A, B, U)`, up to eight values.
#[doc(hidden)]
pub trait AppendValue<U> {
    type Output: PluckColumns;
}

macro_rules! append_value_tuples {
    ($(($($ty:ident),+)),* $(,)?) => {
        $(
            impl<$($ty: PluckColumn,)+ U: PluckColumn> AppendValue<U> for ($($ty,)+) {
                type Output = ($($ty,)+ U);
            }
        )*
    };
}

append_value_tuples!((A, B), (A, B, C), (A, B, C, D), (A, B, C, D, E), (A, B, C, D, E, F), (A, B, C, D, E, F, G));

impl<M, T> PluckAggregates<M, T>
where
    M: DinocoEntity,
    T: PluckColumns,
{
    /// Computes one more aggregate, read back after the others.
    pub fn pluck<F, U>(self, callback: F) -> PluckAggregates<M, T::Output>
    where
        F: FnOnce(M::Where) -> Aggregate<U>,
        U: PluckColumn,
        T: AppendValue<U>,
    {
        let mut aggregates = self.aggregates;
        aggregates.push(callback(M::Where::default()).column());

        PluckAggregates {
            query: self.query,
            aggregates,
            read_primary: self.read_primary,
            cursor: self.cursor,
            marker: PhantomData,
        }
    }

    /// Computes every aggregate once per group of rows sharing a key, read
    /// back as `Vec<(key, (values...))>`. Ordered and paged as
    /// [`PluckAggregate::group_by`], where `order_by` on an aggregated field
    /// orders by that field's first aggregate.
    pub fn group_by<F, G>(self, callback: F) -> PluckGroups<M, G::Key, T>
    where
        F: FnOnce(M::Where) -> G,
        G: IntoGroupKey,
    {
        let key = callback(M::Where::default()).into_group_by();
        let (query, unordered_field, cursor_error) = grouped_query(self.query, self.cursor, self.aggregates, key);

        PluckGroups { query, unordered_field, cursor_error, read_primary: self.read_primary, marker: PhantomData }
    }

    pub fn read_in_primary(mut self) -> Self {
        self.read_primary = true;

        self
    }

    pub fn execute<C>(self, client: C) -> impl Future<Output = anyhow::Result<T>>
    where
        C: ReadExecutor,
    {
        let Self { mut query, aggregates, read_primary, cursor, .. } = self;
        if let Err(error) = apply_cursor(cursor, &mut query) {
            return Either::Left(std::future::ready(Err(error)));
        }
        let query = AggregateQuery { source: query, aggregates, group: None };

        Either::Right(async move {
            let target = client.read_target(read_primary)?;
            let mut rows = target.aggregate::<PluckValues<T>>(query).await?;

            match rows.pop() {
                Some(PluckValues(values)) => Ok(values),
                None => anyhow::bail!("the aggregates over `{}` returned no row", M::TABLE_NAME),
            }
        })
    }
}

/// Several aggregates per group, from `.pluck(...).pluck(...).group_by(...)`:
/// one `(K, T)` per group of rows sharing a key, `T` the tuple of values.
pub struct PluckGroups<M, K, T> {
    query: AggregateQuery,
    unordered_field: Option<&'static str>,
    cursor_error: Option<anyhow::Error>,
    read_primary: bool,
    marker: PhantomData<fn() -> (M, K, T)>,
}

impl<M, K, T> PluckGroups<M, K, T>
where
    M: DinocoEntity,
    K: PluckColumn,
    T: PluckColumns,
{
    pub fn read_in_primary(mut self) -> Self {
        self.read_primary = true;

        self
    }

    pub fn execute<C>(self, client: C) -> impl Future<Output = anyhow::Result<Vec<(K, T)>>>
    where
        C: ReadExecutor,
    {
        let Self { query, unordered_field, cursor_error, read_primary, .. } = self;

        if let Some(error) = cursor_error {
            return Either::Left(std::future::ready(Err(error)));
        }
        if let Some(field) = unordered_field {
            return Either::Left(std::future::ready(Err(anyhow::anyhow!(
                "a grouped pluck on `{}` can only be ordered by the grouped or an aggregated field, not by `{field}`",
                M::TABLE_NAME
            ))));
        }

        Either::Right(async move {
            let target = client.read_target(read_primary)?;
            let rows = target.aggregate::<GroupedValues<K, T>>(query).await?;

            Ok(rows.into_iter().map(|GroupedValues(key, values)| (key, values)).collect())
        })
    }
}

/// The query of a grouped pluck: the find's rows (past its cursor, if any),
/// grouped by `key`, with the find's `order_by`, `take` and `skip` read as
/// the groups' (see [`aggregate_group`]). Also returns the field ordered by
/// that names no column of the groups, and why the cursor failed.
fn grouped_query(
    mut query: FindQuery,
    cursor: Option<Box<KeysetCursor>>,
    aggregates: AggregateList,
    key: GroupBy,
) -> (AggregateQuery, Option<&'static str>, Option<anyhow::Error>) {
    // The cursor picks rows in the find's order, before that order is read
    // as the groups' order below.
    let cursor_error = cursor.and_then(|cursor| cursor.apply(&mut query, false).err());
    let FindQuery { from, conditions, limit, skip, order_by, .. } = query;
    let (group, unordered_field) = aggregate_group(key, &aggregates, order_by.as_slice(), limit, skip);
    let mut source = FindQuery::new(&[], from, -1, -1);
    source.conditions = conditions;

    (AggregateQuery { source, aggregates, group: Some(group) }, unordered_field, cursor_error)
}

/// The groups of a grouped aggregate keyed by `key`: `orders` (the builder's
/// `order_by`) name the key or an aggregated field (its first aggregate), and
/// `limit`/`skip` page the groups. Also returns the first field ordered by
/// that is neither, which the groups have no column for.
pub(crate) fn aggregate_group(
    key: GroupBy,
    aggregates: &AggregateList,
    orders: &[FindOrderBy],
    limit: i32,
    skip: i32,
) -> (AggregateGroup, Option<&'static str>) {
    let group_order = |order: &FindOrderBy| {
        let field = order.field();
        if field == key.field() {
            Some((if order.is_descending() { GroupOrder::KeyDesc } else { GroupOrder::KeyAsc }, 0))
        } else {
            let position = aggregates.position(field)?;
            Some((if order.is_descending() { GroupOrder::AggregateDesc } else { GroupOrder::AggregateAsc }, position))
        }
    };
    let unordered_field = orders.iter().find(|order| group_order(order).is_none()).map(|order| order.field());
    let mut group_orders = orders.iter().filter_map(group_order);
    let first = group_orders.next();
    let then_by = group_orders.next().map(|(order, _)| order);

    let group = AggregateGroup {
        key,
        order_by: first.map(|(order, _)| order),
        then_by,
        // At most eight aggregates: `AppendValue` stops there.
        ordered_aggregate: first.map_or(0, |(_, position)| u8::try_from(position).unwrap_or(u8::MAX)),
        limit,
        skip,
    };
    (group, unordered_field)
}

pub struct FindManyTransform<M, S, F, C = NoCount> {
    inner: FindMany<M, S, C>,
    transform: F,
}

impl<M, S, F, R> FindManyTransform<M, S, F>
where
    M: DinocoEntity + DinocoRowModel,
    S: DinocoRowModel,
    F: FnMut(S) -> R,
{
    pub fn execute<C>(self, client: C) -> impl Future<Output = anyhow::Result<Vec<R>>>
    where
        C: ReadExecutor,
    {
        let transform = self.transform;

        self.inner.execute(client).map_ok(|rows| rows.into_iter().map(transform).collect())
    }
}

impl<M, S, F, R, CF> FindManyTransform<M, S, F, WithCount<CF>>
where
    M: DinocoEntity + DinocoRowModel,
    M::Count: DinocoCountModel<M>,
    S: DinocoRowModel,
    F: FnMut(S) -> R,
    CF: CountFilters<M>,
{
    pub fn execute<C>(self, client: C) -> impl Future<Output = anyhow::Result<(Vec<R>, M::Count)>>
    where
        C: ReadExecutor,
    {
        let transform = self.transform;

        self.inner.execute(client).map_ok(|(rows, count)| (rows.into_iter().map(transform).collect(), count))
    }
}

pub fn find_many<M: DinocoEntity + DinocoRowModel>() -> FindMany<M> {
    FindMany::<M> {
        query: FindQuery::new(M::FIELDS, M::TABLE_NAME, -1, -1),
        includes: Vec::new(),
        read_primary: false,
        complex_where: false,
        count: NoCount,
        cursor: None,
        select_marker: PhantomData,
        marker: PhantomData,
    }
}
