use chrono::{DateTime, NaiveDate, NaiveDateTime, Utc};

use crate::{DeadpoolPostgresRow, DinocoMysql, DinocoPostgres, DinocoSqlite, MysqlRow, PostgresRow, SqliteRow};

/// Wraps a single scalar column read from position `0` of a result row.
///
/// Backs both [`crate::DinocoAdapter::query`]-style scalar projections such as
/// `.pluck(...)` and single-value aggregates such as `exists(...)`, so the
/// same [`crate::DinocoRowModel`] machinery used for entities can decode a
/// bare `SELECT <field> ...` / `SELECT EXISTS(...) ...` result.
#[derive(Debug, Clone, PartialEq)]
pub struct PluckValue<T>(pub T);

macro_rules! impl_pluck_scalar {
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

            impl DinocoMysql for PluckValue<$ty> {
                fn from_mysql_row(row: &MysqlRow) -> Option<Self> {
                    let mut row = row.clone();
                    row.take::<$ty, _>(0).map(PluckValue)
                }
            }

            impl DinocoMysql for PluckValue<Option<$ty>> {
                fn from_mysql_row(row: &MysqlRow) -> Option<Self> {
                    let mut row = row.clone();
                    row.take::<Option<$ty>, _>(0).map(PluckValue)
                }
            }
        )*
    };
}

impl_pluck_scalar!(String, bool, i64, f64, Vec<u8>, NaiveDate, crate::serde_json::Value);

impl DinocoSqlite for PluckValue<DateTime<Utc>> {
    fn from_sqlite_row(row: &SqliteRow<'_>) -> Option<Self> {
        row.get::<_, DateTime<Utc>>(0).ok().map(PluckValue)
    }
}

impl DinocoSqlite for PluckValue<Option<DateTime<Utc>>> {
    fn from_sqlite_row(row: &SqliteRow<'_>) -> Option<Self> {
        row.get::<_, Option<DateTime<Utc>>>(0).ok().map(PluckValue)
    }
}

impl DinocoPostgres for PluckValue<DateTime<Utc>> {
    fn from_deadpool_posgres_row(row: &DeadpoolPostgresRow) -> Option<Self> {
        row.try_get::<_, DateTime<Utc>>(0).ok().map(PluckValue)
    }

    fn from_postgres_row(row: &PostgresRow) -> Option<Self> {
        Self::from_deadpool_posgres_row(row)
    }
}

impl DinocoPostgres for PluckValue<Option<DateTime<Utc>>> {
    fn from_deadpool_posgres_row(row: &DeadpoolPostgresRow) -> Option<Self> {
        row.try_get::<_, Option<DateTime<Utc>>>(0).ok().map(PluckValue)
    }

    fn from_postgres_row(row: &PostgresRow) -> Option<Self> {
        Self::from_deadpool_posgres_row(row)
    }
}

impl DinocoMysql for PluckValue<DateTime<Utc>> {
    fn from_mysql_row(row: &MysqlRow) -> Option<Self> {
        let mut row = row.clone();
        row.take::<NaiveDateTime, _>(0).map(|value| PluckValue(value.and_utc()))
    }
}

impl DinocoMysql for PluckValue<Option<DateTime<Utc>>> {
    fn from_mysql_row(row: &MysqlRow) -> Option<Self> {
        let mut row = row.clone();
        row.take::<Option<NaiveDateTime>, _>(0).map(|value| PluckValue(value.map(|value| value.and_utc())))
    }
}
