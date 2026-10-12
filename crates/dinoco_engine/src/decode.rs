//! Column lookup and value helpers used by the code `#[derive(Entity)]`
//! generates to decode rows.
//!
//! Dinoco selects a model's columns in declaration order, so the column of a
//! field is almost always at the field's position. The lookups confirm that by
//! comparing a single name and only scan the row (with each driver's own
//! name-matching rules) when the statement was written by hand.

#[cfg(feature = "postgres")]
use chrono::{DateTime, NaiveDateTime, Utc};
#[cfg(feature = "mysql")]
use mysql_common::value::convert::FromValue;
#[cfg(feature = "postgres")]
use tokio_postgres::types::Type;

#[cfg(feature = "mysql")]
use crate::MysqlRow;
#[cfg(feature = "postgres")]
use crate::PostgresRow;
#[cfg(feature = "sqlite")]
use crate::SqliteRow;

/// Index of the `name` column of a SQLite row, expected at `index`.
#[cfg(feature = "sqlite")]
#[doc(hidden)]
#[inline]
pub fn sqlite_column(row: &SqliteRow<'_>, index: usize, name: &str) -> Option<usize> {
    let statement = row.as_ref();
    if statement.column_name(index).is_ok_and(|column| column == name) {
        return Some(index);
    }

    statement.column_index(name).ok()
}

/// Index of the `name` column of a PostgreSQL row, expected at `index`.
#[cfg(feature = "postgres")]
#[doc(hidden)]
#[inline]
pub fn postgres_column(row: &PostgresRow, index: usize, name: &str) -> Option<usize> {
    let columns = row.columns();
    if columns.get(index).is_some_and(|column| column.name() == name) {
        return Some(index);
    }

    columns
        .iter()
        .position(|column| column.name() == name)
        .or_else(|| columns.iter().position(|column| column.name().eq_ignore_ascii_case(name)))
}

/// Index of the `name` column of a MySQL row, expected at `index`.
#[cfg(feature = "mysql")]
#[doc(hidden)]
#[inline]
pub fn mysql_column(row: &MysqlRow, index: usize, name: &str) -> Option<usize> {
    let columns = row.columns_ref();
    if columns.get(index).is_some_and(|column| column.name_ref() == name.as_bytes()) {
        return Some(index);
    }

    columns.iter().position(|column| column.name_ref() == name.as_bytes())
}

/// Moves the value at `index` out of `row`. `None` when the value is missing
/// or cannot be converted to `T` (the driver's `take` panics instead).
#[cfg(feature = "mysql")]
#[doc(hidden)]
#[inline]
pub fn mysql_take<T: FromValue>(row: &mut MysqlRow, index: usize) -> Option<T> {
    row.take_opt(index)?.ok()
}

/// A `DateTime<Utc>` read from a `TIMESTAMPTZ` column, or from a `TIMESTAMP`
/// column holding a UTC wall clock value.
#[cfg(feature = "postgres")]
#[doc(hidden)]
pub fn postgres_datetime_at(row: &PostgresRow, index: usize) -> Option<DateTime<Utc>> {
    if *row.columns().get(index)?.type_() == Type::TIMESTAMPTZ {
        row.try_get::<_, DateTime<Utc>>(index).ok()
    } else {
        row.try_get::<_, NaiveDateTime>(index).ok().map(|value| value.and_utc())
    }
}

/// [`postgres_datetime_at`] for a nullable column.
#[cfg(feature = "postgres")]
#[doc(hidden)]
pub fn postgres_optional_datetime_at(row: &PostgresRow, index: usize) -> Option<Option<DateTime<Utc>>> {
    if *row.columns().get(index)?.type_() == Type::TIMESTAMPTZ {
        row.try_get::<_, Option<DateTime<Utc>>>(index).ok()
    } else {
        row.try_get::<_, Option<NaiveDateTime>>(index).ok().map(|value| value.map(|value| value.and_utc()))
    }
}

#[cfg(feature = "postgres")]
#[doc(hidden)]
pub fn postgres_datetime_from_row(row: &PostgresRow, field: &str) -> Option<DateTime<Utc>> {
    postgres_datetime_at(row, postgres_column(row, 0, field)?)
}

#[cfg(feature = "postgres")]
#[doc(hidden)]
pub fn postgres_optional_datetime_from_row(row: &PostgresRow, field: &str) -> Option<Option<DateTime<Utc>>> {
    postgres_optional_datetime_at(row, postgres_column(row, 0, field)?)
}
