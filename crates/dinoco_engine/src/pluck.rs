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

/// Scalars every database decodes directly.
macro_rules! scalars {
    ($impl:ident) => {
        $impl!(String, bool, i64, f64, Vec<u8>, NaiveDate, crate::serde_json::Value);
    };
}

#[cfg(feature = "sqlite")]
mod sqlite {
    use super::*;
    use crate::{DinocoSqlite, SqliteRow};

    macro_rules! impl_sqlite {
        ($($ty:ty),* $(,)?) => {
            $(
                impl DinocoSqlite for PluckValue<$ty> {
                    fn from_sqlite_row(row: &SqliteRow<'_>) -> Option<Self> {
                        row.get::<_, $ty>(0).ok().map(PluckValue)
                    }
                }

                impl DinocoSqlite for PluckValue<Option<$ty>> {
                    fn from_sqlite_row(row: &SqliteRow<'_>) -> Option<Self> {
                        row.get::<_, Option<$ty>>(0).ok().map(PluckValue)
                    }
                }
            )*
        };
    }

    scalars!(impl_sqlite);
    impl_sqlite!(DateTime<Utc>);
}

#[cfg(feature = "postgres")]
mod postgres {
    use super::*;
    use crate::{DeadpoolPostgresRow, DinocoPostgres, PostgresRow};

    macro_rules! impl_postgres {
        ($($ty:ty),* $(,)?) => {
            $(
                impl DinocoPostgres for PluckValue<$ty> {
                    fn from_deadpool_posgres_row(row: &DeadpoolPostgresRow) -> Option<Self> {
                        row.try_get::<_, $ty>(0).ok().map(PluckValue)
                    }

                    fn from_postgres_row(row: &PostgresRow) -> Option<Self> {
                        Self::from_deadpool_posgres_row(row)
                    }
                }

                impl DinocoPostgres for PluckValue<Option<$ty>> {
                    fn from_deadpool_posgres_row(row: &DeadpoolPostgresRow) -> Option<Self> {
                        row.try_get::<_, Option<$ty>>(0).ok().map(PluckValue)
                    }

                    fn from_postgres_row(row: &PostgresRow) -> Option<Self> {
                        Self::from_deadpool_posgres_row(row)
                    }
                }
            )*
        };
    }

    scalars!(impl_postgres);

    /// `DateTime` columns are `TIMESTAMP` holding UTC, or `TIMESTAMPTZ`.
    impl DinocoPostgres for PluckValue<DateTime<Utc>> {
        fn from_deadpool_posgres_row(row: &DeadpoolPostgresRow) -> Option<Self> {
            crate::postgres_datetime_at(row, 0).map(PluckValue)
        }

        fn from_postgres_row(row: &PostgresRow) -> Option<Self> {
            Self::from_deadpool_posgres_row(row)
        }
    }

    impl DinocoPostgres for PluckValue<Option<DateTime<Utc>>> {
        fn from_deadpool_posgres_row(row: &DeadpoolPostgresRow) -> Option<Self> {
            crate::postgres_optional_datetime_at(row, 0).map(PluckValue)
        }

        fn from_postgres_row(row: &PostgresRow) -> Option<Self> {
            Self::from_deadpool_posgres_row(row)
        }
    }
}

#[cfg(feature = "mysql")]
mod mysql {
    use chrono::NaiveDateTime;

    use super::*;
    use crate::{DinocoMysql, MysqlRow};

    macro_rules! impl_mysql {
        ($($ty:ty),* $(,)?) => {
            $(
                impl DinocoMysql for PluckValue<$ty> {
                    fn from_mysql_row(row: &MysqlRow) -> Option<Self> {
                        row.get_opt::<$ty, _>(0)?.ok().map(PluckValue)
                    }

                    fn from_mysql_row_owned(mut row: MysqlRow) -> Option<Self> {
                        crate::mysql_take::<$ty>(&mut row, 0).map(PluckValue)
                    }
                }

                impl DinocoMysql for PluckValue<Option<$ty>> {
                    fn from_mysql_row(row: &MysqlRow) -> Option<Self> {
                        row.get_opt::<Option<$ty>, _>(0)?.ok().map(PluckValue)
                    }

                    fn from_mysql_row_owned(mut row: MysqlRow) -> Option<Self> {
                        crate::mysql_take::<Option<$ty>>(&mut row, 0).map(PluckValue)
                    }
                }
            )*
        };
    }

    scalars!(impl_mysql);

    /// `DateTime` columns hold UTC wall clock values.
    impl DinocoMysql for PluckValue<DateTime<Utc>> {
        fn from_mysql_row(row: &MysqlRow) -> Option<Self> {
            row.get_opt::<NaiveDateTime, _>(0)?.ok().map(|value| PluckValue(value.and_utc()))
        }
    }

    impl DinocoMysql for PluckValue<Option<DateTime<Utc>>> {
        fn from_mysql_row(row: &MysqlRow) -> Option<Self> {
            row.get_opt::<Option<NaiveDateTime>, _>(0)?.ok().map(|value| PluckValue(value.map(|value| value.and_utc())))
        }
    }
}
