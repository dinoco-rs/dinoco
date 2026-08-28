use std::str::FromStr;
use std::sync::Arc;

use anyhow::{Context, anyhow};
use deadpool_postgres::{ManagerConfig, Pool, RecyclingMethod, Runtime};
use tokio_postgres::types::{IsNull, Json, Kind, ToSql, Type};
use tokio_postgres::{Config, NoTls};

mod compiler;

use crate::{
    CompiledTransactionCommand, CompiledTransactionStatement, DinocoAdapter, DinocoRowModel, DinocoValue,
    LiveTransactionMessage, RawTransactionOutput, RowDecodeError, TransactionCommandKind,
};

#[derive(Clone, Copy, Debug)]
pub enum PostgresMode {
    Direct,
    PgBouncer,
}

pub const DEFAULT_MIN_CONNECTIONS: usize = 2;
pub const DEFAULT_MAX_CONNECTIONS: usize = 10;

#[derive(Clone)]
pub struct PostgresAdapter {
    pub url: String,
    pub pool: Arc<Pool>,
    pub mode: PostgresMode,
    with_logger: bool,
}

pub struct PgBouncerAdapter {
    inner: PostgresAdapter,
}

#[async_trait::async_trait]
impl DinocoAdapter for PostgresAdapter {
    async fn new(url: String) -> Result<Self, String> {
        Self::direct(url).await.map_err(|err| err.to_string())
    }

    async fn query<M>(&self, query: &str, params: &[DinocoValue]) -> anyhow::Result<Vec<M>>
    where
        M: DinocoRowModel,
    {
        let conn = self.pool.get().await.context("Failed to get postgres connection from pool")?;
        let params = postgres_params(params);
        let params = postgres_param_refs(&params);
        let rows = match self.mode {
            PostgresMode::Direct => {
                let stmt = conn.prepare_cached(query).await?;
                conn.query(&stmt, &params).await?
            }
            PostgresMode::PgBouncer => conn.query(query, &params).await?,
        };

        rows.into_iter()
            .map(|row| {
                M::from_deadpool_posgres_row(&row).ok_or_else(|| RowDecodeError::new(std::any::type_name::<M>()).into())
            })
            .collect()
    }

    async fn query_optional<M>(&self, query: &str, params: &[DinocoValue]) -> anyhow::Result<Vec<M>>
    where
        M: DinocoRowModel,
    {
        let conn = self.pool.get().await.context("Failed to get postgres connection from pool")?;
        let params = postgres_params(params);
        let params = postgres_param_refs(&params);
        let rows = match self.mode {
            PostgresMode::Direct => {
                let stmt = conn.prepare_cached(query).await?;
                conn.query(&stmt, &params).await?
            }
            PostgresMode::PgBouncer => conn.query(query, &params).await?,
        };

        Ok(rows.into_iter().filter_map(|row| M::from_deadpool_posgres_row(&row)).collect())
    }

    async fn execute(&self, query: &str, params: &[DinocoValue]) -> anyhow::Result<usize> {
        let conn = self.pool.get().await.context("Failed to get postgres connection from pool")?;
        let params = postgres_params(params);
        let params = postgres_param_refs(&params);
        let affected = match self.mode {
            PostgresMode::Direct => {
                let stmt = conn.prepare_cached(query).await?;
                conn.execute(&stmt, &params).await?
            }
            PostgresMode::PgBouncer => conn.execute(query, &params).await?,
        };

        Ok(affected as usize)
    }
}

impl PostgresAdapter {
    pub async fn direct(url: impl Into<String>) -> anyhow::Result<Self> {
        Self::direct_with_pool(url, DEFAULT_MIN_CONNECTIONS, DEFAULT_MAX_CONNECTIONS).await
    }

    pub async fn direct_with_pool(
        url: impl Into<String>,
        min_connections: usize,
        max_connections: usize,
    ) -> anyhow::Result<Self> {
        if min_connections == 0 {
            anyhow::bail!("PostgreSQL min_connections must be greater than zero");
        }
        if max_connections == 0 {
            anyhow::bail!("PostgreSQL max_connections must be greater than zero");
        }
        if min_connections > max_connections {
            anyhow::bail!(
                "PostgreSQL min_connections ({min_connections}) cannot be greater than max_connections ({max_connections})"
            );
        }

        Self::from_url(url.into(), PostgresMode::Direct, Some((min_connections, max_connections))).await
    }

    pub async fn pgbouncer(url: impl Into<String>) -> anyhow::Result<Self> {
        Self::from_url(url.into(), PostgresMode::PgBouncer, None).await
    }

    async fn from_url(url: String, mode: PostgresMode, pool_limits: Option<(usize, usize)>) -> anyhow::Result<Self> {
        let pg_config = Config::from_str(&url).context("Invalid postgres url")?;
        let manager_config = ManagerConfig {
            recycling_method: match mode {
                PostgresMode::Direct => RecyclingMethod::Fast,
                PostgresMode::PgBouncer => RecyclingMethod::Fast,
            },
        };
        let manager = deadpool_postgres::Manager::from_config(pg_config, NoTls, manager_config);
        let mut builder = Pool::builder(manager).runtime(Runtime::Tokio1);
        if let Some((_, max_connections)) = pool_limits {
            builder = builder.max_size(max_connections);
        }
        let pool = builder.build()?;

        if let Some((min_connections, _)) = pool_limits {
            let mut warm_connections = Vec::with_capacity(min_connections);
            for _ in 0..min_connections {
                warm_connections
                    .push(pool.get().await.context("Failed to create the configured minimum PostgreSQL connections")?);
            }
        }

        Ok(Self { url, pool: Arc::new(pool), mode, with_logger: false })
    }

    pub(crate) fn set_logger(&mut self, enabled: bool) {
        self.with_logger = enabled;
    }

    pub(crate) fn logger_enabled(&self) -> bool {
        self.with_logger
    }

    pub(crate) async fn begin_live_transaction(
        &self,
    ) -> anyhow::Result<tokio::sync::mpsc::Sender<LiveTransactionMessage>> {
        let pool = self.pool.clone();
        let compiler = self.clone();
        let (sender, mut receiver) = tokio::sync::mpsc::channel(1);
        let (ready_sender, ready_receiver) = tokio::sync::oneshot::channel();

        tokio::spawn(async move {
            let mut connection = match pool.get().await {
                Ok(connection) => connection,
                Err(error) => {
                    let _ = ready_sender.send(Err(anyhow::Error::from(error)));
                    return;
                }
            };
            let transaction = match connection.transaction().await {
                Ok(transaction) => transaction,
                Err(error) => {
                    let _ = ready_sender.send(Err(anyhow::Error::from(error)));
                    return;
                }
            };
            let _ = ready_sender.send(Ok(()));

            while let Some(message) = receiver.recv().await {
                match message {
                    LiveTransactionMessage::Execute { command, reply } => {
                        let result = match command.compile(&compiler) {
                            Ok(command) => execute_transaction_command(&transaction, &command)
                                .await
                                .and_then(|raw| command.finish(raw)),
                            Err(error) => Err(error),
                        };
                        let _ = reply.send(result);
                    }
                    LiveTransactionMessage::Commit { reply } => {
                        let result = transaction.commit().await.map_err(anyhow::Error::from);
                        let _ = reply.send(result);
                        return;
                    }
                    LiveTransactionMessage::Rollback { reply } => {
                        let result = transaction.rollback().await.map_err(anyhow::Error::from);
                        let _ = reply.send(result);
                        return;
                    }
                }
            }
        });

        ready_receiver.await.map_err(|_| anyhow!("postgres transaction worker stopped during BEGIN"))??;
        Ok(sender)
    }

    pub async fn query_count(&self, query: &str, params: &[DinocoValue]) -> anyhow::Result<i64> {
        let conn = self.pool.get().await.context("Failed to get postgres connection from pool")?;
        let params = postgres_params(params);
        let params = postgres_param_refs(&params);
        let row = match self.mode {
            PostgresMode::Direct => {
                let stmt = conn.prepare_cached(query).await?;
                conn.query_one(&stmt, &params).await?
            }
            PostgresMode::PgBouncer => conn.query_one(query, &params).await?,
        };

        Ok(row.try_get(0)?)
    }
}

async fn execute_transaction_command(
    transaction: &tokio_postgres::Transaction<'_>,
    command: &CompiledTransactionCommand,
) -> anyhow::Result<RawTransactionOutput> {
    let mut output = None;
    for statement in &command.statements {
        let raw = execute_transaction_statement(transaction, statement).await?;
        if statement.output {
            output = Some(raw);
        }
    }

    output.ok_or_else(|| anyhow!("Dinoco transaction command contains no output statement."))
}

async fn execute_transaction_statement(
    transaction: &tokio_postgres::Transaction<'_>,
    command: &CompiledTransactionStatement,
) -> anyhow::Result<RawTransactionOutput> {
    if command.sql.is_empty() {
        return match command.kind {
            TransactionCommandKind::Rows => Ok(RawTransactionOutput::Rows(Vec::new())),
            TransactionCommandKind::Execute => Ok(RawTransactionOutput::Affected(0)),
        };
    }

    let params = postgres_params(&command.params);
    let params = postgres_param_refs(&params);

    match command.kind {
        TransactionCommandKind::Rows => {
            let decoder = command
                .decoder
                .ok_or_else(|| anyhow!("Dinoco transaction query is missing its postgres row decoder."))?;
            let rows = transaction.query(command.sql.as_str(), &params).await?;
            let values = rows
                .iter()
                .map(|row| (decoder.postgres)(row).ok_or_else(|| RowDecodeError::new("transaction result").into()))
                .collect::<anyhow::Result<Vec<_>>>()?;
            Ok(RawTransactionOutput::Rows(values))
        }
        TransactionCommandKind::Execute => {
            let affected = transaction.execute(command.sql.as_str(), &params).await?;
            Ok(RawTransactionOutput::Affected(affected as usize))
        }
    }
}

#[async_trait::async_trait]
impl DinocoAdapter for PgBouncerAdapter {
    async fn new(url: String) -> Result<Self, String> {
        Self::new(url).await.map_err(|err| err.to_string())
    }

    async fn query<M>(&self, query: &str, params: &[DinocoValue]) -> anyhow::Result<Vec<M>>
    where
        M: DinocoRowModel,
    {
        self.inner.query(query, params).await
    }

    async fn query_optional<M>(&self, query: &str, params: &[DinocoValue]) -> anyhow::Result<Vec<M>>
    where
        M: DinocoRowModel,
    {
        self.inner.query_optional(query, params).await
    }

    async fn execute(&self, query: &str, params: &[DinocoValue]) -> anyhow::Result<usize> {
        self.inner.execute(query, params).await
    }
}

impl PgBouncerAdapter {
    pub async fn new(url: impl Into<String>) -> anyhow::Result<Self> {
        Ok(Self { inner: PostgresAdapter::pgbouncer(url).await? })
    }

    pub fn inner(&self) -> &PostgresAdapter {
        &self.inner
    }

    pub(crate) fn set_logger(&mut self, enabled: bool) {
        self.inner.set_logger(enabled);
    }

    pub(crate) fn logger_enabled(&self) -> bool {
        self.inner.logger_enabled()
    }

    pub async fn query_count(&self, query: &str, params: &[DinocoValue]) -> anyhow::Result<i64> {
        self.inner.query_count(query, params).await
    }
}

fn postgres_params(params: &[DinocoValue]) -> Vec<Box<dyn ToSql + Sync + Send>> {
    params
        .iter()
        .map(|param| match param {
            DinocoValue::Null => Box::new(PostgresNull) as Box<dyn ToSql + Sync + Send>,
            DinocoValue::Integer(value) => Box::new(*value) as Box<dyn ToSql + Sync + Send>,
            DinocoValue::Float(value) => Box::new(*value) as Box<dyn ToSql + Sync + Send>,
            DinocoValue::String(value) => Box::new(value.clone()) as Box<dyn ToSql + Sync + Send>,
            DinocoValue::Enum(name, value) => {
                Box::new(PostgresEnumValue { name: name.clone(), value: value.clone() }) as Box<dyn ToSql + Sync + Send>
            }
            DinocoValue::Boolean(value) => Box::new(*value) as Box<dyn ToSql + Sync + Send>,
            DinocoValue::Bytes(value) => Box::new(value.clone()) as Box<dyn ToSql + Sync + Send>,
            DinocoValue::Json(value) => Box::new(Json(value.clone())) as Box<dyn ToSql + Sync + Send>,
            DinocoValue::DateTime(value) => Box::new(PostgresDateTimeValue(*value)) as Box<dyn ToSql + Sync + Send>,
            DinocoValue::Date(value) => Box::new(*value) as Box<dyn ToSql + Sync + Send>,
        })
        .collect()
}

#[derive(Debug)]
struct PostgresNull;

impl ToSql for PostgresNull {
    fn to_sql(
        &self,
        _ty: &Type,
        _out: &mut tokio_postgres::types::private::BytesMut,
    ) -> Result<IsNull, Box<dyn std::error::Error + Sync + Send>> {
        Ok(IsNull::Yes)
    }

    fn accepts(_ty: &Type) -> bool {
        true
    }

    tokio_postgres::types::to_sql_checked!();
}

#[derive(Debug)]
struct PostgresEnumValue {
    name: String,
    value: String,
}

#[derive(Debug)]
struct PostgresDateTimeValue(chrono::DateTime<chrono::Utc>);

impl ToSql for PostgresDateTimeValue {
    fn to_sql(
        &self,
        ty: &Type,
        out: &mut tokio_postgres::types::private::BytesMut,
    ) -> Result<IsNull, Box<dyn std::error::Error + Sync + Send>> {
        if *ty == Type::TIMESTAMP {
            return <chrono::NaiveDateTime as ToSql>::to_sql(&self.0.naive_utc(), ty, out);
        }
        if *ty == Type::TIMESTAMPTZ {
            return <chrono::DateTime<chrono::Utc> as ToSql>::to_sql(&self.0, ty, out);
        }

        Err(format!("DateTime<Utc> cannot be written to PostgreSQL type `{}`", ty.name()).into())
    }

    fn accepts(ty: &Type) -> bool {
        *ty == Type::TIMESTAMP || *ty == Type::TIMESTAMPTZ
    }

    tokio_postgres::types::to_sql_checked!();
}

impl ToSql for PostgresEnumValue {
    fn to_sql(
        &self,
        ty: &Type,
        out: &mut tokio_postgres::types::private::BytesMut,
    ) -> Result<IsNull, Box<dyn std::error::Error + Sync + Send>> {
        if matches!(ty.kind(), Kind::Enum(_)) && !ty.name().eq_ignore_ascii_case(&self.name) {
            return Err(format!("enum `{}` cannot be written to PostgreSQL enum `{}`", self.name, ty.name()).into());
        }
        <&str as ToSql>::to_sql(&self.value.as_str(), ty, out)
    }

    fn accepts(ty: &Type) -> bool {
        matches!(ty.kind(), Kind::Enum(_)) || <String as ToSql>::accepts(ty)
    }

    tokio_postgres::types::to_sql_checked!();
}

fn postgres_param_refs(params: &[Box<dyn ToSql + Sync + Send>]) -> Vec<&(dyn ToSql + Sync)> {
    params.iter().map(|param| param.as_ref() as &(dyn ToSql + Sync)).collect()
}

#[doc(hidden)]
pub fn postgres_datetime_from_row(row: &tokio_postgres::Row, field: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    row.try_get::<_, chrono::DateTime<chrono::Utc>>(field)
        .or_else(|_| {
            row.try_get::<_, chrono::NaiveDateTime>(field)
                .map(|value| chrono::DateTime::from_naive_utc_and_offset(value, chrono::Utc))
        })
        .ok()
}

#[doc(hidden)]
pub fn postgres_optional_datetime_from_row(
    row: &tokio_postgres::Row,
    field: &str,
) -> Option<Option<chrono::DateTime<chrono::Utc>>> {
    row.try_get::<_, Option<chrono::DateTime<chrono::Utc>>>(field)
        .or_else(|_| {
            row.try_get::<_, Option<chrono::NaiveDateTime>>(field)
                .map(|value| value.map(|value| chrono::DateTime::from_naive_utc_and_offset(value, chrono::Utc)))
        })
        .ok()
}

impl<'a> tokio_postgres::types::FromSql<'a> for DinocoValue {
    fn from_sql(
        ty: &tokio_postgres::types::Type,
        raw: &'a [u8],
    ) -> Result<Self, Box<dyn std::error::Error + Sync + Send>> {
        if *ty == tokio_postgres::types::Type::BOOL {
            return Ok(DinocoValue::Boolean(<bool as tokio_postgres::types::FromSql>::from_sql(ty, raw)?));
        }

        if *ty == tokio_postgres::types::Type::FLOAT4 {
            return Ok(DinocoValue::Float(<f32 as tokio_postgres::types::FromSql>::from_sql(ty, raw)? as f64));
        }

        if *ty == tokio_postgres::types::Type::FLOAT8 {
            return Ok(DinocoValue::Float(<f64 as tokio_postgres::types::FromSql>::from_sql(ty, raw)?));
        }

        if *ty == tokio_postgres::types::Type::INT2 {
            return Ok(DinocoValue::Integer(<i16 as tokio_postgres::types::FromSql>::from_sql(ty, raw)? as i64));
        }

        if *ty == tokio_postgres::types::Type::INT4 {
            return Ok(DinocoValue::Integer(<i32 as tokio_postgres::types::FromSql>::from_sql(ty, raw)? as i64));
        }

        if *ty == tokio_postgres::types::Type::INT8 {
            return Ok(DinocoValue::Integer(<i64 as tokio_postgres::types::FromSql>::from_sql(ty, raw)?));
        }

        if *ty == tokio_postgres::types::Type::BYTEA {
            return Ok(DinocoValue::Bytes(<Vec<u8> as tokio_postgres::types::FromSql>::from_sql(ty, raw)?));
        }

        if *ty == tokio_postgres::types::Type::JSON || *ty == tokio_postgres::types::Type::JSONB {
            return Ok(DinocoValue::Json(<serde_json::Value as tokio_postgres::types::FromSql>::from_sql(ty, raw)?));
        }

        if *ty == tokio_postgres::types::Type::TIMESTAMPTZ {
            return Ok(DinocoValue::DateTime(
                <chrono::DateTime<chrono::Utc> as tokio_postgres::types::FromSql>::from_sql(ty, raw)?,
            ));
        }

        if *ty == tokio_postgres::types::Type::TIMESTAMP {
            let naive = <chrono::NaiveDateTime as tokio_postgres::types::FromSql>::from_sql(ty, raw)?;
            return Ok(DinocoValue::DateTime(chrono::DateTime::from_naive_utc_and_offset(naive, chrono::Utc)));
        }

        if *ty == tokio_postgres::types::Type::DATE {
            return Ok(DinocoValue::Date(<chrono::NaiveDate as tokio_postgres::types::FromSql>::from_sql(ty, raw)?));
        }

        Ok(DinocoValue::String(<String as tokio_postgres::types::FromSql>::from_sql(ty, raw)?))
    }

    fn accepts(_ty: &tokio_postgres::types::Type) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn postgres_enum_and_null_parameters_accept_native_enum_columns() {
        let ty = Type::new(
            "AuthMethod".to_string(),
            99_999,
            Kind::Enum(vec!["PASSWORD".to_string(), "GOOGLE".to_string()]),
            "public".to_string(),
        );
        let params =
            postgres_params(&[DinocoValue::Enum("AuthMethod".to_string(), "GOOGLE".to_string()), DinocoValue::Null]);

        let mut enum_bytes = tokio_postgres::types::private::BytesMut::new();
        assert!(params[0].to_sql_checked(&ty, &mut enum_bytes).is_ok());
        assert_eq!(enum_bytes.as_ref(), b"GOOGLE");

        let mut null_bytes = tokio_postgres::types::private::BytesMut::new();
        assert!(matches!(params[1].to_sql_checked(&ty, &mut null_bytes), Ok(IsNull::Yes)));
        assert!(null_bytes.is_empty());
    }

    #[test]
    fn postgres_datetime_parameters_accept_timestamp_with_and_without_timezone() {
        let value = chrono::DateTime::from_timestamp(1_700_000_000, 123_456_000).expect("valid timestamp");
        let params = postgres_params(&[DinocoValue::DateTime(value)]);

        let mut timestamp_bytes = tokio_postgres::types::private::BytesMut::new();
        assert!(params[0].to_sql_checked(&Type::TIMESTAMP, &mut timestamp_bytes).is_ok());
        assert!(!timestamp_bytes.is_empty());

        let mut timestamptz_bytes = tokio_postgres::types::private::BytesMut::new();
        assert!(params[0].to_sql_checked(&Type::TIMESTAMPTZ, &mut timestamptz_bytes).is_ok());
        assert!(!timestamptz_bytes.is_empty());

        let mut date_bytes = tokio_postgres::types::private::BytesMut::new();
        assert!(params[0].to_sql_checked(&Type::DATE, &mut date_bytes).is_err());
    }
}
