//! Aggregates and group keys for `find_many(...).pluck(...)`, built from the
//! typed fields of a `Where`.

use std::marker::PhantomData;

use dinoco_engine::chrono::{DateTime, NaiveDate, Utc};
use dinoco_engine::{AggregateColumn, AggregateFunction, DateBucket, GroupBy, PluckColumn};

use crate::Field;

/// An aggregate the database computes over one column, built by a field's
/// `sum`, `avg`, `min`, `max` or `count` and passed to
/// `find_many(...).pluck(...)`. `T` is the type it is read back as.
///
/// ```ignore
/// let revenue = find_many::<Order>()
///     .where_(|order| order.paid.eq(true))
///     .pluck(|order| order.amount.sum())
///     .execute(&client)
///     .await?; // i64
/// ```
pub struct Aggregate<T> {
    column: AggregateColumn,
    marker: PhantomData<fn() -> T>,
}

impl<T> Clone for Aggregate<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for Aggregate<T> {}

impl<T> Aggregate<T> {
    const fn new(function: AggregateFunction, field: &'static str) -> Self {
        Self { column: AggregateColumn { function, field }, marker: PhantomData }
    }

    /// The function and the column it reads.
    pub const fn column(&self) -> AggregateColumn {
        self.column
    }
}

/// What `.group_by(...)` splits rows by: a field's own values, or the date,
/// week, month or year a date field falls in, or the hour or minute of a
/// timestamp. `K` is the type of the key read back with each group.
///
/// ```ignore
/// .group_by(|order| order.status)             // Status
/// .group_by(|order| order.created_at.date())  // NaiveDate
/// .group_by(|order| order.created_at.hour())  // DateTime<Utc>
/// ```
pub struct GroupKey<K> {
    key: GroupBy,
    marker: PhantomData<fn() -> K>,
}

impl<K> Clone for GroupKey<K> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<K> Copy for GroupKey<K> {}

impl<K> GroupKey<K> {
    const fn bucket(field: &'static str, bucket: DateBucket) -> Self {
        Self { key: GroupBy::Date(field, bucket), marker: PhantomData }
    }
}

/// A group key: a [`Field`] groups by its values, a [`GroupKey`] by the date
/// bucket it names.
pub trait IntoGroupKey {
    type Key: PluckColumn;

    #[doc(hidden)]
    fn into_group_by(self) -> GroupBy;
}

impl<T, C> IntoGroupKey for Field<T, C>
where
    T: PluckColumn,
{
    type Key = T;

    fn into_group_by(self) -> GroupBy {
        GroupBy::Field(self.field_name())
    }
}

impl<K> IntoGroupKey for GroupKey<K>
where
    K: PluckColumn,
{
    type Key = K;

    fn into_group_by(self) -> GroupBy {
        self.key
    }
}

/// Field types that can be summed and averaged: `i64` and `f64`, optional or
/// not. A sum is read back as the non-optional type.
pub trait NumericValue {
    #[doc(hidden)]
    type Sum: PluckColumn;
    #[doc(hidden)]
    const SUM: AggregateFunction;
}

/// Field types with an order, for `min` and `max`: numbers, strings, dates and
/// timestamps, optional or not. `Value` is the type without the `Option`.
pub trait OrderedValue {
    #[doc(hidden)]
    type Value: PluckColumn;
}

/// Date and timestamp field types, which can be grouped by day, week, month
/// or year. Each bucket is read back as the date it starts on, optional when
/// the field is.
pub trait DateValue {
    #[doc(hidden)]
    type Bucket: PluckColumn;
}

macro_rules! numeric_values {
    ($($ty:ty => $function:ident),* $(,)?) => {
        $(
            impl NumericValue for $ty {
                type Sum = $ty;
                const SUM: AggregateFunction = AggregateFunction::$function;
            }

            impl NumericValue for Option<$ty> {
                type Sum = $ty;
                const SUM: AggregateFunction = AggregateFunction::$function;
            }
        )*
    };
}

numeric_values!(i64 => SumInteger, f64 => SumFloat);

macro_rules! ordered_values {
    ($($ty:ty),* $(,)?) => {
        $(
            impl OrderedValue for $ty {
                type Value = $ty;
            }

            impl OrderedValue for Option<$ty> {
                type Value = $ty;
            }
        )*
    };
}

ordered_values!(i64, f64, String, NaiveDate, DateTime<Utc>);

/// Timestamp field types, which can also be grouped by hour or minute. Each
/// bucket is read back as the timestamp it starts at, optional when the field
/// is.
pub trait TimestampValue {
    #[doc(hidden)]
    type Truncated: PluckColumn;
}

impl TimestampValue for DateTime<Utc> {
    type Truncated = DateTime<Utc>;
}

impl TimestampValue for Option<DateTime<Utc>> {
    type Truncated = Option<DateTime<Utc>>;
}

impl DateValue for DateTime<Utc> {
    type Bucket = NaiveDate;
}

impl DateValue for Option<DateTime<Utc>> {
    type Bucket = Option<NaiveDate>;
}

impl DateValue for NaiveDate {
    type Bucket = NaiveDate;
}

impl DateValue for Option<NaiveDate> {
    type Bucket = Option<NaiveDate>;
}

impl<T, C> Field<T, C> {
    /// `COUNT` of the aggregated rows whose column is not `NULL`.
    pub fn count(self) -> Aggregate<i64> {
        Aggregate::new(AggregateFunction::Count, self.field_name())
    }
}

impl<T, C> Field<T, C>
where
    T: NumericValue,
{
    /// `SUM` of the column; `0` when no row, or only `NULL`, is aggregated.
    pub fn sum(self) -> Aggregate<T::Sum> {
        Aggregate::new(T::SUM, self.field_name())
    }

    /// `AVG` of the column, as a float; `None` when no row, or only `NULL`,
    /// is aggregated.
    pub fn avg(self) -> Aggregate<Option<f64>> {
        Aggregate::new(AggregateFunction::Avg, self.field_name())
    }
}

impl<T, C> Field<T, C>
where
    T: OrderedValue,
{
    /// `MIN` of the column; `None` when no row, or only `NULL`, is aggregated.
    pub fn min(self) -> Aggregate<Option<T::Value>> {
        Aggregate::new(AggregateFunction::Min, self.field_name())
    }

    /// `MAX` of the column; `None` when no row, or only `NULL`, is aggregated.
    pub fn max(self) -> Aggregate<Option<T::Value>> {
        Aggregate::new(AggregateFunction::Max, self.field_name())
    }
}

impl<T, C> Field<T, C>
where
    T: DateValue,
{
    /// Groups by the full date of the value: its day, month and year.
    pub fn date(self) -> GroupKey<T::Bucket> {
        GroupKey::bucket(self.field_name(), DateBucket::Day)
    }

    /// The same as [`date`](Self::date).
    pub fn day(self) -> GroupKey<T::Bucket> {
        self.date()
    }

    /// Groups by the Monday of the value's week.
    pub fn week(self) -> GroupKey<T::Bucket> {
        GroupKey::bucket(self.field_name(), DateBucket::Week)
    }

    /// Groups by the first day of the value's month.
    pub fn month(self) -> GroupKey<T::Bucket> {
        GroupKey::bucket(self.field_name(), DateBucket::Month)
    }

    /// Groups by January 1st of the value's year.
    pub fn year(self) -> GroupKey<T::Bucket> {
        GroupKey::bucket(self.field_name(), DateBucket::Year)
    }
}

impl<T, C> Field<T, C>
where
    T: TimestampValue,
{
    /// Groups by the hour of the value: its date and hour, at minute zero.
    pub fn hour(self) -> GroupKey<T::Truncated> {
        GroupKey::bucket(self.field_name(), DateBucket::Hour)
    }

    /// Groups by the minute of the value: its date, hour and minute, at second
    /// zero.
    pub fn minute(self) -> GroupKey<T::Truncated> {
        GroupKey::bucket(self.field_name(), DateBucket::Minute)
    }
}
