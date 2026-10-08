use std::marker::PhantomData;

use dinoco_engine::{
    DinocoEntity, DinocoValue, FindWhere, ManyToManyMatch, RelationJoinTable, RelationMatch, RelationQuantifier,
    WhereComplex,
};

/// Capability generated only for fields declared with `@fulltext`.
///
/// Regular string fields intentionally do not expose full-text search.
pub struct FullTextField;

pub struct Field<T, Capability = ()> {
    name: &'static str,
    fulltext_fields: &'static [&'static str],
    marker: PhantomData<fn() -> (T, Capability)>,
}

impl<T, Capability> Clone for Field<T, Capability> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T, Capability> Copy for Field<T, Capability> {}

impl<T, Capability> Field<T, Capability> {
    pub const fn new(name: &'static str) -> Self {
        Self { name, fulltext_fields: &[], marker: PhantomData }
    }

    #[doc(hidden)]
    pub const fn new_fulltext(name: &'static str, fulltext_fields: &'static [&'static str]) -> Self {
        Self { name, fulltext_fields, marker: PhantomData }
    }

    /// The underlying column name, used by `.pluck(...)` to project a single
    /// field instead of building a [`FindWhere`] predicate.
    pub const fn field_name(&self) -> &'static str {
        self.name
    }

    pub fn eq<V>(self, value: V) -> FindWhere
    where
        V: Into<DinocoValue>,
    {
        FindWhere::Eq(self.name, value.into())
    }

    pub fn neq<V>(self, value: V) -> FindWhere
    where
        V: Into<DinocoValue>,
    {
        FindWhere::Neq(self.name, value.into())
    }

    pub fn gt<V>(self, value: V) -> FindWhere
    where
        V: Into<DinocoValue>,
    {
        FindWhere::Gt(self.name, value.into())
    }

    pub fn gte<V>(self, value: V) -> FindWhere
    where
        V: Into<DinocoValue>,
    {
        FindWhere::Gte(self.name, value.into())
    }

    pub fn lt<V>(self, value: V) -> FindWhere
    where
        V: Into<DinocoValue>,
    {
        FindWhere::Lt(self.name, value.into())
    }

    pub fn lte<V>(self, value: V) -> FindWhere
    where
        V: Into<DinocoValue>,
    {
        FindWhere::Lte(self.name, value.into())
    }

    pub fn batch<I, V>(self, values: I) -> FindWhere
    where
        I: IntoIterator<Item = V>,
        V: Into<DinocoValue>,
    {
        FindWhere::Batch(self.name, values.into_iter().map(Into::into).collect())
    }

    pub fn null(self) -> FindWhere {
        FindWhere::Null(self.name)
    }

    pub fn not_null(self) -> FindWhere {
        FindWhere::NotNull(self.name)
    }
}

macro_rules! impl_string_field {
    ($($ty:ty),* $(,)?) => {
        $(
            impl<Capability> Field<$ty, Capability> {
                pub fn like<V>(self, value: V) -> FindWhere
                where
                    V: AsRef<str>,
                {
                    FindWhere::Like(self.name, DinocoValue::String(format!("%{}%", value.as_ref())))
                }

                pub fn starts_with<V>(self, value: V) -> FindWhere
                where
                    V: AsRef<str>,
                {
                    FindWhere::Like(self.name, DinocoValue::String(format!("{}%", value.as_ref())))
                }

                pub fn ends_with<V>(self, value: V) -> FindWhere
                where
                    V: AsRef<str>,
                {
                    FindWhere::Like(self.name, DinocoValue::String(format!("%{}", value.as_ref())))
                }
            }
        )*
    };
}

macro_rules! impl_between_field {
    ($($ty:ty),* $(,)?) => {
        $(
            impl Field<$ty> {
                pub fn between<V>(self, start: V, end: V) -> FindWhere
                where
                    V: Into<DinocoValue>,
                {
                    FindWhere::Between(self.name, start.into(), end.into())
                }
            }
        )*
    };
}

impl_string_field!(String, Option<String>);

impl Field<String, FullTextField> {
    pub fn fulltext<V>(self, value: V) -> FindWhere
    where
        V: AsRef<str>,
    {
        FindWhere::FullText(self.fulltext_fields, DinocoValue::String(value.as_ref().to_string()))
    }
}

impl Field<Option<String>, FullTextField> {
    pub fn fulltext<V>(self, value: V) -> FindWhere
    where
        V: AsRef<str>,
    {
        FindWhere::FullText(self.fulltext_fields, DinocoValue::String(value.as_ref().to_string()))
    }
}
/// Query capability generated for a `#[dinoco(many_to_many_key)]` virtual
/// field (`Option<PrimaryKey>`). It never maps to a real column: every method
/// builds a [`FindWhere::ManyToMany`] membership test so a `find`/`count` on
/// one side of a many-to-many relation can be filtered by a predicate on the
/// id of a row on the other side.
///
/// The full [`Field`] filter surface is available and is evaluated against the
/// join table's target column: `eq`/`neq`, `gt`/`gte`/`lt`/`lte`, `batch`,
/// `null`/`not_null`, `like`/`starts_with`/`ends_with` (string keys), and
/// `between` (numeric/date keys). `neq` and `not_in` negate membership
/// (`NOT IN`), so they also keep rows with no link at all; every other method
/// keeps rows linked to *some* row matching the predicate.
///
/// ```ignore
/// // Only the accounts linked to `business_id` through the join table.
/// find_many::<Account>()
///     .where_(|account| account.business_id.eq(&business_id))
///     .execute(&client)
///     .await?;
/// ```
pub struct ManyToManyKeyField<T> {
    local_key: &'static str,
    join_table: &'static str,
    join_local_field: &'static str,
    join_target_field: &'static str,
    marker: PhantomData<fn() -> T>,
}

impl<T> Clone for ManyToManyKeyField<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for ManyToManyKeyField<T> {}

impl<T> ManyToManyKeyField<T> {
    /// Mirrors [`crate::ManyToManyUpdateField::new`] so the derive can build
    /// both from the same attribute list.
    pub const fn new(
        _name: &'static str,
        join_table: &'static str,
        parent_field: &'static str,
        join_parent_field: &'static str,
        join_child_field: &'static str,
    ) -> Self {
        Self {
            local_key: parent_field,
            join_table,
            join_local_field: join_parent_field,
            join_target_field: join_child_field,
            marker: PhantomData,
        }
    }

    /// The join table's target column as a plain [`Field`], so every filter
    /// method can be reused verbatim before wrapping it in the membership test.
    fn target(self) -> Field<T> {
        Field::new(self.join_target_field)
    }

    /// Wraps a predicate over the target column into the membership subquery.
    fn wrap(self, predicate: FindWhere, negated: bool) -> FindWhere {
        FindWhere::ManyToMany(ManyToManyMatch {
            local_key: self.local_key,
            join_table: self.join_table,
            join_local_field: self.join_local_field,
            join_target_field: self.join_target_field,
            negated,
            predicate: Box::new(predicate),
        })
    }

    /// Keeps rows linked to `value`.
    pub fn eq<V>(self, value: V) -> FindWhere
    where
        V: Into<DinocoValue>,
    {
        self.wrap(self.target().eq(value), false)
    }

    /// Keeps rows *not* linked to `value` (including rows with no link at all).
    pub fn neq<V>(self, value: V) -> FindWhere
    where
        V: Into<DinocoValue>,
    {
        self.wrap(self.target().eq(value), true)
    }

    /// Keeps rows linked to some row whose key is greater than `value`.
    pub fn gt<V>(self, value: V) -> FindWhere
    where
        V: Into<DinocoValue>,
    {
        self.wrap(self.target().gt(value), false)
    }

    /// Keeps rows linked to some row whose key is greater than or equal to `value`.
    pub fn gte<V>(self, value: V) -> FindWhere
    where
        V: Into<DinocoValue>,
    {
        self.wrap(self.target().gte(value), false)
    }

    /// Keeps rows linked to some row whose key is less than `value`.
    pub fn lt<V>(self, value: V) -> FindWhere
    where
        V: Into<DinocoValue>,
    {
        self.wrap(self.target().lt(value), false)
    }

    /// Keeps rows linked to some row whose key is less than or equal to `value`.
    pub fn lte<V>(self, value: V) -> FindWhere
    where
        V: Into<DinocoValue>,
    {
        self.wrap(self.target().lte(value), false)
    }

    /// Keeps rows linked to at least one of `values`.
    pub fn batch<I, V>(self, values: I) -> FindWhere
    where
        I: IntoIterator<Item = V>,
        V: Into<DinocoValue>,
    {
        self.wrap(self.target().batch(values), false)
    }

    /// Keeps rows linked to none of `values` (including rows with no link at all).
    pub fn not_in<I, V>(self, values: I) -> FindWhere
    where
        I: IntoIterator<Item = V>,
        V: Into<DinocoValue>,
    {
        self.wrap(self.target().batch(values), true)
    }

    /// Keeps rows linked through a join row whose target column is `NULL`.
    pub fn null(self) -> FindWhere {
        self.wrap(self.target().null(), false)
    }

    /// Keeps rows that have at least one link (target column not `NULL`).
    pub fn not_null(self) -> FindWhere {
        self.wrap(self.target().not_null(), false)
    }
}

macro_rules! impl_many_to_many_string_field {
    ($($ty:ty),* $(,)?) => {
        $(
            impl ManyToManyKeyField<$ty> {
                /// Keeps rows linked to some row whose key contains `value`.
                pub fn like<V>(self, value: V) -> FindWhere
                where
                    V: AsRef<str>,
                {
                    self.wrap(self.target().like(value), false)
                }

                /// Keeps rows linked to some row whose key starts with `value`.
                pub fn starts_with<V>(self, value: V) -> FindWhere
                where
                    V: AsRef<str>,
                {
                    self.wrap(self.target().starts_with(value), false)
                }

                /// Keeps rows linked to some row whose key ends with `value`.
                pub fn ends_with<V>(self, value: V) -> FindWhere
                where
                    V: AsRef<str>,
                {
                    self.wrap(self.target().ends_with(value), false)
                }
            }
        )*
    };
}

impl_many_to_many_string_field!(String, Option<String>);

/// Query capability generated in `Where` for every relation field. It filters
/// the queried rows by their related rows without loading them: each method
/// compiles to a correlated `EXISTS` subquery, so it needs no `.includes(...)`
/// and works in every builder that takes a `where_` — finds, `count`,
/// `exists`, updates, deletes, include and count-include filters, and inside
/// `where_complex` trees and transactions. Callbacks receive the related
/// model's own `Where`, so relation filters nest.
///
/// For a singular relation "related rows" is the one row the foreign key
/// points at (or that points back, for the non-owning side of a one-to-one),
/// if any. Conditions inside one callback must hold on the same related row;
/// separate relation filters may each be satisfied by a different one.
///
/// ```ignore
/// // Transactions paid out through Pix; `payout` itself stays unloaded.
/// find_many::<Transaction>()
///     .where_(|transaction| transaction.payout.where_(|payout| payout.method.eq(PayoutMethod::Pix)))
///     .execute(&client)
///     .await?;
/// ```
pub struct RelationField<C> {
    parent_table: &'static str,
    parent_field: &'static str,
    child_field: &'static str,
    join: Option<RelationJoinTable>,
    marker: PhantomData<fn() -> C>,
}

impl<C> Clone for RelationField<C> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<C> Copy for RelationField<C> {}

impl<C> RelationField<C> {
    /// A relation matched on `parent_table.parent_field = <related>.child_field`.
    /// Takes the same keys as the relation's include builder.
    pub const fn new(parent_table: &'static str, parent_field: &'static str, child_field: &'static str) -> Self {
        Self { parent_table, parent_field, child_field, join: None, marker: PhantomData }
    }

    /// A many-to-many relation, reached through `join_table`.
    pub const fn many_to_many(
        parent_table: &'static str,
        parent_field: &'static str,
        child_field: &'static str,
        join_table: &'static str,
        join_parent_field: &'static str,
        join_child_field: &'static str,
    ) -> Self {
        Self {
            parent_table,
            parent_field,
            child_field,
            join: Some(RelationJoinTable {
                table: join_table,
                parent_field: join_parent_field,
                child_field: join_child_field,
            }),
            marker: PhantomData,
        }
    }
}

impl<C> RelationField<C>
where
    C: DinocoEntity,
{
    fn matching(self, quantifier: RelationQuantifier, conditions: Vec<FindWhere>) -> FindWhere {
        FindWhere::Relation(RelationMatch {
            parent_table: self.parent_table,
            parent_field: self.parent_field,
            child_table: C::TABLE_NAME,
            child_field: self.child_field,
            join: self.join,
            quantifier,
            conditions,
        })
    }

    /// Keeps rows with at least one related row matching `callback`.
    pub fn where_<F>(self, callback: F) -> FindWhere
    where
        F: FnOnce(C::Where) -> FindWhere,
    {
        self.matching(RelationQuantifier::Some, vec![callback(C::Where::default())])
    }

    /// [`where_`](Self::where_) with a [`WhereComplex`] tree over the related row.
    pub fn where_complex<F>(self, callback: F) -> FindWhere
    where
        F: FnOnce(C::Where, WhereComplex) -> FindWhere,
    {
        self.matching(RelationQuantifier::Some, vec![callback(C::Where::default(), WhereComplex)])
    }

    /// Keeps rows with no related row matching `callback`, including rows
    /// without any related row.
    pub fn none<F>(self, callback: F) -> FindWhere
    where
        F: FnOnce(C::Where) -> FindWhere,
    {
        self.matching(RelationQuantifier::None, vec![callback(C::Where::default())])
    }

    /// Keeps rows whose related rows all match `callback`, including rows
    /// without any related row. A related row whose condition evaluates to
    /// `NULL` (a comparison against a `NULL` column) does not match.
    pub fn every<F>(self, callback: F) -> FindWhere
    where
        F: FnOnce(C::Where) -> FindWhere,
    {
        self.matching(RelationQuantifier::Every, vec![callback(C::Where::default())])
    }

    /// Keeps rows with at least one related row.
    pub fn exists(self) -> FindWhere {
        self.matching(RelationQuantifier::Some, Vec::new())
    }

    /// Keeps rows without any related row.
    pub fn not_exists(self) -> FindWhere {
        self.matching(RelationQuantifier::None, Vec::new())
    }
}

macro_rules! impl_many_to_many_between_field {
    ($($ty:ty),* $(,)?) => {
        $(
            impl ManyToManyKeyField<$ty> {
                /// Keeps rows linked to some row whose key is within `[start, end]`.
                pub fn between<V>(self, start: V, end: V) -> FindWhere
                where
                    V: Into<DinocoValue>,
                {
                    self.wrap(self.target().between(start, end), false)
                }
            }
        )*
    };
}

impl_many_to_many_between_field!(
    i8,
    i16,
    i32,
    i64,
    i128,
    isize,
    u8,
    u16,
    u32,
    u64,
    u128,
    usize,
    f32,
    f64,
    dinoco_engine::chrono::DateTime<dinoco_engine::chrono::Utc>,
    dinoco_engine::chrono::NaiveDate,
);

impl_between_field!(
    i8,
    i16,
    i32,
    i64,
    i128,
    isize,
    u8,
    u16,
    u32,
    u64,
    u128,
    usize,
    f32,
    f64,
    Option<i8>,
    Option<i16>,
    Option<i32>,
    Option<i64>,
    Option<i128>,
    Option<isize>,
    Option<u8>,
    Option<u16>,
    Option<u32>,
    Option<u64>,
    Option<u128>,
    Option<usize>,
    Option<f32>,
    Option<f64>,
    dinoco_engine::chrono::DateTime<dinoco_engine::chrono::Utc>,
    dinoco_engine::chrono::NaiveDate,
    Option<dinoco_engine::chrono::DateTime<dinoco_engine::chrono::Utc>>,
    Option<dinoco_engine::chrono::NaiveDate>,
);
