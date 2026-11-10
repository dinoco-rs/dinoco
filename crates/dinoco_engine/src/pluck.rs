use std::any::{Any, TypeId};

#[cfg(any(feature = "sqlite", feature = "postgres", feature = "mysql"))]
use chrono::{DateTime, NaiveDate, Utc};

/// Wraps a single scalar column read from position `0` of a result row.
///
/// Backs both [`crate::DinocoAdapter::query`]-style scalar projections such as
/// `.pluck(...)` and single-value aggregates such as `exists(...)`, so the
/// same [`crate::DinocoRowModel`] machinery used for entities can decode a
/// bare `SELECT <field> ...` / `SELECT EXISTS(...) ...` result.
#[derive(Debug, Clone, PartialEq)]
pub struct PluckValue<T>(pub T);

/// A group key and its aggregate, read from positions `0` and `1` of a row of
/// a grouped [`crate::AggregateQuery`].
#[derive(Debug, Clone, PartialEq)]
pub struct GroupedValue<K, T>(pub K, pub T);

/// The values of several aggregates, read from consecutive columns starting
/// at position `0` of a row of an [`crate::AggregateQuery`].
#[derive(Debug, Clone, PartialEq)]
pub struct PluckValues<T>(pub T);

/// A group key and the values of several aggregates, read from position `0`
/// and the consecutive columns after it.
#[derive(Debug, Clone, PartialEq)]
pub struct GroupedValues<K, T>(pub K, pub T);

/// Several scalars read from consecutive columns of a row: a tuple of two to
/// eight [`PluckColumn`]s, the values of an aggregate pluck over several
/// fields.
pub trait PluckColumns: Sized + Send + Sync + 'static {
    /// How many columns the tuple reads.
    const LEN: usize;

    #[cfg(feature = "sqlite")]
    #[doc(hidden)]
    fn from_sqlite_columns(row: &crate::SqliteRow<'_>, start: usize) -> Option<Self>;

    #[cfg(feature = "postgres")]
    #[doc(hidden)]
    fn from_postgres_columns(row: &crate::PostgresRow, start: usize) -> Option<Self>;

    #[cfg(feature = "mysql")]
    #[doc(hidden)]
    fn from_mysql_columns(row: &mut crate::MysqlRow, start: usize) -> Option<Self>;

    /// Hands each value, by position, to `store`, which writes it into a
    /// model field through the [`PluckColumn::dinoco_store`] it is given.
    /// Returns whether every value was stored.
    #[doc(hidden)]
    fn dinoco_store_each(self, store: &mut dyn FnMut(usize, &mut dyn FnMut(&mut dyn Any) -> bool) -> bool) -> bool;

    /// Whether the value at `position` can be stored in `slot`.
    #[doc(hidden)]
    fn dinoco_fits_at(position: usize, slot: &dyn Any) -> bool;

    /// The type of the value at `position`, for error messages.
    #[doc(hidden)]
    fn dinoco_type_name_at(position: usize) -> &'static str;
}

macro_rules! pluck_column_tuples {
    ($(($($ty:ident: $index:tt),+)),* $(,)?) => {
        $(
            impl<$($ty: PluckColumn),+> PluckColumns for ($($ty,)+) {
                const LEN: usize = [$($index),+].len();

                #[cfg(feature = "sqlite")]
                fn from_sqlite_columns(row: &crate::SqliteRow<'_>, start: usize) -> Option<Self> {
                    Some(($($ty::from_sqlite_column(row, start + $index)?,)+))
                }

                #[cfg(feature = "postgres")]
                fn from_postgres_columns(row: &crate::PostgresRow, start: usize) -> Option<Self> {
                    Some(($($ty::from_postgres_column(row, start + $index)?,)+))
                }

                #[cfg(feature = "mysql")]
                fn from_mysql_columns(row: &mut crate::MysqlRow, start: usize) -> Option<Self> {
                    Some(($($ty::from_mysql_column(row, start + $index)?,)+))
                }

                fn dinoco_store_each(
                    self,
                    store: &mut dyn FnMut(usize, &mut dyn FnMut(&mut dyn Any) -> bool) -> bool,
                ) -> bool {
                    let mut stored = true;
                    $(
                        let mut value = Some(self.$index);
                        stored &= store($index, &mut |slot| value.take().is_some_and(|value| value.dinoco_store(slot)));
                    )+
                    stored
                }

                fn dinoco_fits_at(position: usize, slot: &dyn Any) -> bool {
                    match position {
                        $($index => $ty::dinoco_fits(slot),)+
                        _ => false,
                    }
                }

                fn dinoco_type_name_at(position: usize) -> &'static str {
                    match position {
                        $($index => std::any::type_name::<$ty>(),)+
                        _ => "",
                    }
                }
            }
        )*
    };
}

pluck_column_tuples!(
    (A: 0, B: 1),
    (A: 0, B: 1, C: 2),
    (A: 0, B: 1, C: 2, D: 3),
    (A: 0, B: 1, C: 2, D: 3, E: 4),
    (A: 0, B: 1, C: 2, D: 3, E: 4, F: 5),
    (A: 0, B: 1, C: 2, D: 3, E: 4, F: 5, G: 6),
    (A: 0, B: 1, C: 2, D: 3, E: 4, F: 5, G: 6, H: 7),
);

/// A scalar read from one column of a row, by position.
///
/// Implemented for every scalar type a model field can have (and for enums
/// deriving `DinocoEnum`), plus `Option` of each. [`PluckValue`] and
/// [`GroupedValue`] decode their columns through it.
pub trait PluckColumn: Sized + Send + Sync + 'static {
    #[cfg(feature = "sqlite")]
    #[doc(hidden)]
    fn from_sqlite_column(row: &crate::SqliteRow<'_>, index: usize) -> Option<Self>;

    /// The column as an `Option`, `None` for `NULL`.
    #[cfg(feature = "sqlite")]
    #[doc(hidden)]
    fn from_sqlite_column_nullable(row: &crate::SqliteRow<'_>, index: usize) -> Option<Option<Self>>;

    #[cfg(feature = "postgres")]
    #[doc(hidden)]
    fn from_postgres_column(row: &crate::PostgresRow, index: usize) -> Option<Self>;

    #[cfg(feature = "postgres")]
    #[doc(hidden)]
    fn from_postgres_column_nullable(row: &crate::PostgresRow, index: usize) -> Option<Option<Self>>;

    /// Moves the value out of a row Dinoco owns.
    #[cfg(feature = "mysql")]
    #[doc(hidden)]
    fn from_mysql_column(row: &mut crate::MysqlRow, index: usize) -> Option<Self>;

    #[cfg(feature = "mysql")]
    #[doc(hidden)]
    fn from_mysql_column_nullable(row: &mut crate::MysqlRow, index: usize) -> Option<Option<Self>>;

    /// Writes the value into `slot`, a model field of type `Self` or
    /// `Option<Self>` (a date into a `DateTime<Utc>` field, at midnight UTC).
    /// Returns `false`, leaving the field as it was, when it has another type.
    #[doc(hidden)]
    fn dinoco_store(self, slot: &mut dyn Any) -> bool {
        if let Some(slot) = slot.downcast_mut::<Self>() {
            *slot = self;
            return true;
        }
        if let Some(slot) = slot.downcast_mut::<Option<Self>>() {
            *slot = Some(self);
            return true;
        }

        let Some(date) = (&self as &dyn Any).downcast_ref::<chrono::NaiveDate>() else {
            return false;
        };
        let midnight = date.and_time(chrono::NaiveTime::MIN).and_utc();
        if let Some(slot) = slot.downcast_mut::<chrono::DateTime<chrono::Utc>>() {
            *slot = midnight;
            true
        } else if let Some(slot) = slot.downcast_mut::<Option<chrono::DateTime<chrono::Utc>>>() {
            *slot = Some(midnight);
            true
        } else {
            false
        }
    }

    /// Whether [`dinoco_store`](Self::dinoco_store) can write into `slot`.
    #[doc(hidden)]
    fn dinoco_fits(slot: &dyn Any) -> bool {
        slot.is::<Self>()
            || slot.is::<Option<Self>>()
            || (TypeId::of::<Self>() == TypeId::of::<chrono::NaiveDate>()
                && (slot.is::<chrono::DateTime<chrono::Utc>>() || slot.is::<Option<chrono::DateTime<chrono::Utc>>>()))
    }
}

impl<T: PluckColumn> PluckColumn for Option<T> {
    /// A `NULL` leaves a non-optional field at its default.
    fn dinoco_store(self, slot: &mut dyn Any) -> bool {
        if let Some(slot) = slot.downcast_mut::<Self>() {
            *slot = self;
            return true;
        }

        match self {
            Some(value) => value.dinoco_store(slot),
            None => T::dinoco_fits(slot),
        }
    }

    fn dinoco_fits(slot: &dyn Any) -> bool {
        slot.is::<Self>() || T::dinoco_fits(slot)
    }

    #[cfg(feature = "sqlite")]
    fn from_sqlite_column(row: &crate::SqliteRow<'_>, index: usize) -> Option<Self> {
        T::from_sqlite_column_nullable(row, index)
    }

    #[cfg(feature = "sqlite")]
    fn from_sqlite_column_nullable(row: &crate::SqliteRow<'_>, index: usize) -> Option<Option<Self>> {
        T::from_sqlite_column_nullable(row, index).map(Some)
    }

    #[cfg(feature = "postgres")]
    fn from_postgres_column(row: &crate::PostgresRow, index: usize) -> Option<Self> {
        T::from_postgres_column_nullable(row, index)
    }

    #[cfg(feature = "postgres")]
    fn from_postgres_column_nullable(row: &crate::PostgresRow, index: usize) -> Option<Option<Self>> {
        T::from_postgres_column_nullable(row, index).map(Some)
    }

    #[cfg(feature = "mysql")]
    fn from_mysql_column(row: &mut crate::MysqlRow, index: usize) -> Option<Self> {
        T::from_mysql_column_nullable(row, index)
    }

    #[cfg(feature = "mysql")]
    fn from_mysql_column_nullable(row: &mut crate::MysqlRow, index: usize) -> Option<Option<Self>> {
        T::from_mysql_column_nullable(row, index).map(Some)
    }
}

/// Implements [`PluckColumn`] for types each driver decodes directly
/// (`rusqlite::FromSql`, `tokio_postgres::FromSql`, `mysql_common::FromValue`).
/// `DinocoEnum` expands it for every enum.
#[doc(hidden)]
#[macro_export]
macro_rules! __dinoco_pluck_column {
    ($($ty:ty),* $(,)?) => {
        $(
            impl $crate::PluckColumn for $ty {
                $crate::__dinoco_sqlite! {
                    fn from_sqlite_column(row: &$crate::SqliteRow<'_>, index: usize) -> ::core::option::Option<Self> {
                        row.get::<_, $ty>(index).ok()
                    }

                    fn from_sqlite_column_nullable(
                        row: &$crate::SqliteRow<'_>,
                        index: usize,
                    ) -> ::core::option::Option<::core::option::Option<Self>> {
                        row.get::<_, ::core::option::Option<$ty>>(index).ok()
                    }
                }

                $crate::__dinoco_postgres! {
                    fn from_postgres_column(row: &$crate::PostgresRow, index: usize) -> ::core::option::Option<Self> {
                        row.try_get::<_, $ty>(index).ok()
                    }

                    fn from_postgres_column_nullable(
                        row: &$crate::PostgresRow,
                        index: usize,
                    ) -> ::core::option::Option<::core::option::Option<Self>> {
                        row.try_get::<_, ::core::option::Option<$ty>>(index).ok()
                    }
                }

                $crate::__dinoco_mysql! {
                    fn from_mysql_column(row: &mut $crate::MysqlRow, index: usize) -> ::core::option::Option<Self> {
                        $crate::mysql_take::<$ty>(row, index)
                    }

                    fn from_mysql_column_nullable(
                        row: &mut $crate::MysqlRow,
                        index: usize,
                    ) -> ::core::option::Option<::core::option::Option<Self>> {
                        $crate::mysql_take::<::core::option::Option<$ty>>(row, index)
                    }
                }
            }
        )*
    };
}

#[cfg(any(feature = "sqlite", feature = "postgres", feature = "mysql"))]
crate::__dinoco_pluck_column!(String, bool, i64, f64, Vec<u8>, NaiveDate, crate::serde_json::Value);

/// `DateTime` columns are RFC 3339 text on SQLite, `TIMESTAMP` holding UTC (or
/// `TIMESTAMPTZ`) on PostgreSQL, and UTC wall clock values on MySQL.
#[cfg(any(feature = "sqlite", feature = "postgres", feature = "mysql"))]
impl PluckColumn for DateTime<Utc> {
    #[cfg(feature = "sqlite")]
    fn from_sqlite_column(row: &crate::SqliteRow<'_>, index: usize) -> Option<Self> {
        row.get::<_, DateTime<Utc>>(index).ok()
    }

    #[cfg(feature = "sqlite")]
    fn from_sqlite_column_nullable(row: &crate::SqliteRow<'_>, index: usize) -> Option<Option<Self>> {
        row.get::<_, Option<DateTime<Utc>>>(index).ok()
    }

    #[cfg(feature = "postgres")]
    fn from_postgres_column(row: &crate::PostgresRow, index: usize) -> Option<Self> {
        crate::postgres_datetime_at(row, index)
    }

    #[cfg(feature = "postgres")]
    fn from_postgres_column_nullable(row: &crate::PostgresRow, index: usize) -> Option<Option<Self>> {
        crate::postgres_optional_datetime_at(row, index)
    }

    #[cfg(feature = "mysql")]
    fn from_mysql_column(row: &mut crate::MysqlRow, index: usize) -> Option<Self> {
        crate::mysql_take::<chrono::NaiveDateTime>(row, index).map(|value| value.and_utc())
    }

    #[cfg(feature = "mysql")]
    fn from_mysql_column_nullable(row: &mut crate::MysqlRow, index: usize) -> Option<Option<Self>> {
        crate::mysql_take::<Option<chrono::NaiveDateTime>>(row, index).map(|value| value.map(|value| value.and_utc()))
    }
}

#[cfg(feature = "sqlite")]
mod sqlite {
    use super::*;
    use crate::{DinocoSqlite, SqliteRow};

    impl<T: PluckColumn> DinocoSqlite for PluckValue<T> {
        fn from_sqlite_row(row: &SqliteRow<'_>) -> Option<Self> {
            T::from_sqlite_column(row, 0).map(PluckValue)
        }
    }

    impl<K: PluckColumn, T: PluckColumn> DinocoSqlite for GroupedValue<K, T> {
        fn from_sqlite_row(row: &SqliteRow<'_>) -> Option<Self> {
            Some(GroupedValue(K::from_sqlite_column(row, 0)?, T::from_sqlite_column(row, 1)?))
        }
    }

    impl<T: PluckColumns> DinocoSqlite for PluckValues<T> {
        fn from_sqlite_row(row: &SqliteRow<'_>) -> Option<Self> {
            T::from_sqlite_columns(row, 0).map(PluckValues)
        }
    }

    impl<K: PluckColumn, T: PluckColumns> DinocoSqlite for GroupedValues<K, T> {
        fn from_sqlite_row(row: &SqliteRow<'_>) -> Option<Self> {
            Some(GroupedValues(K::from_sqlite_column(row, 0)?, T::from_sqlite_columns(row, 1)?))
        }
    }
}

#[cfg(feature = "postgres")]
mod postgres {
    use super::*;
    use crate::{DeadpoolPostgresRow, DinocoPostgres, PostgresRow};

    impl<T: PluckColumn> DinocoPostgres for PluckValue<T> {
        fn from_deadpool_posgres_row(row: &DeadpoolPostgresRow) -> Option<Self> {
            T::from_postgres_column(row, 0).map(PluckValue)
        }

        fn from_postgres_row(row: &PostgresRow) -> Option<Self> {
            Self::from_deadpool_posgres_row(row)
        }
    }

    impl<K: PluckColumn, T: PluckColumn> DinocoPostgres for GroupedValue<K, T> {
        fn from_deadpool_posgres_row(row: &DeadpoolPostgresRow) -> Option<Self> {
            Some(GroupedValue(K::from_postgres_column(row, 0)?, T::from_postgres_column(row, 1)?))
        }

        fn from_postgres_row(row: &PostgresRow) -> Option<Self> {
            Self::from_deadpool_posgres_row(row)
        }
    }

    impl<T: PluckColumns> DinocoPostgres for PluckValues<T> {
        fn from_deadpool_posgres_row(row: &DeadpoolPostgresRow) -> Option<Self> {
            T::from_postgres_columns(row, 0).map(PluckValues)
        }

        fn from_postgres_row(row: &PostgresRow) -> Option<Self> {
            Self::from_deadpool_posgres_row(row)
        }
    }

    impl<K: PluckColumn, T: PluckColumns> DinocoPostgres for GroupedValues<K, T> {
        fn from_deadpool_posgres_row(row: &DeadpoolPostgresRow) -> Option<Self> {
            Some(GroupedValues(K::from_postgres_column(row, 0)?, T::from_postgres_columns(row, 1)?))
        }

        fn from_postgres_row(row: &PostgresRow) -> Option<Self> {
            Self::from_deadpool_posgres_row(row)
        }
    }
}

#[cfg(feature = "mysql")]
mod mysql {
    use super::*;
    use crate::{DinocoMysql, MysqlRow};

    impl<T: PluckColumn> DinocoMysql for PluckValue<T> {
        fn from_mysql_row(row: &MysqlRow) -> Option<Self> {
            Self::from_mysql_row_owned(row.clone())
        }

        fn from_mysql_row_owned(mut row: MysqlRow) -> Option<Self> {
            T::from_mysql_column(&mut row, 0).map(PluckValue)
        }
    }

    impl<K: PluckColumn, T: PluckColumn> DinocoMysql for GroupedValue<K, T> {
        fn from_mysql_row(row: &MysqlRow) -> Option<Self> {
            Self::from_mysql_row_owned(row.clone())
        }

        fn from_mysql_row_owned(mut row: MysqlRow) -> Option<Self> {
            Some(GroupedValue(K::from_mysql_column(&mut row, 0)?, T::from_mysql_column(&mut row, 1)?))
        }
    }

    impl<T: PluckColumns> DinocoMysql for PluckValues<T> {
        fn from_mysql_row(row: &MysqlRow) -> Option<Self> {
            Self::from_mysql_row_owned(row.clone())
        }

        fn from_mysql_row_owned(mut row: MysqlRow) -> Option<Self> {
            T::from_mysql_columns(&mut row, 0).map(PluckValues)
        }
    }

    impl<K: PluckColumn, T: PluckColumns> DinocoMysql for GroupedValues<K, T> {
        fn from_mysql_row(row: &MysqlRow) -> Option<Self> {
            Self::from_mysql_row_owned(row.clone())
        }

        fn from_mysql_row_owned(mut row: MysqlRow) -> Option<Self> {
            Some(GroupedValues(K::from_mysql_column(&mut row, 0)?, T::from_mysql_columns(&mut row, 1)?))
        }
    }
}
