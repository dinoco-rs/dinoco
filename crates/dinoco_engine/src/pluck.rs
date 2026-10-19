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
}

impl<T: PluckColumn> PluckColumn for Option<T> {
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
}
