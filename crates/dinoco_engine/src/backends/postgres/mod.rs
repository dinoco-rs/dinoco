use std::sync::Arc;

use anyhow::{Context, anyhow};
use deadpool_postgres::{ManagerConfig, Pool, RecyclingMethod, Runtime};
use futures_util::TryStreamExt;
use tokio_postgres::types::{IsNull, Json, Kind, ToSql, Type, WrongType};
use tokio_postgres::{NoTls, RowStream};

mod compiler;
mod tls;

use super::Decode;
use super::sql::Dialect;
use crate::{
    CompiledTransactionCommand, CompiledTransactionStatement, DinocoAdapter, DinocoPostgres, DinocoRowModel,
    DinocoValue, LiveTransactionMessage, RawTransactionOutput, RowDecodeError, TransactionCommandKind,
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

#[derive(Clone)]
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
        self.fetch(query.to_owned(), params.to_vec(), Decode::Strict).await
    }

    async fn query_optional<M>(&self, query: &str, params: &[DinocoValue]) -> anyhow::Result<Vec<M>>
    where
        M: DinocoRowModel,
    {
        self.fetch(query.to_owned(), params.to_vec(), Decode::Lenient).await
    }

    async fn execute(&self, query: &str, params: &[DinocoValue]) -> anyhow::Result<usize> {
        self.run(query.to_owned(), params.to_vec()).await
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
        let (pg_config, tls_mode) = tls::parse_url(&url)?;
        // `Fast` recycling keeps each connection's prepared statements.
        let manager_config = ManagerConfig { recycling_method: RecyclingMethod::Fast };
        let manager = match tls::connector(tls_mode)? {
            Some(tls) => deadpool_postgres::Manager::from_config(pg_config, tls, manager_config),
            None => deadpool_postgres::Manager::from_config(pg_config, NoTls, manager_config),
        };
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

    async fn connection(&self) -> anyhow::Result<deadpool_postgres::Object> {
        self.pool.get().await.context("Failed to get postgres connection from pool")
    }

    /// The statement to run `sql` with: prepared once per connection and
    /// cached, except behind PgBouncer where named statements cannot be kept.
    async fn statement(
        &self,
        connection: &deadpool_postgres::Object,
        sql: &str,
    ) -> anyhow::Result<Option<tokio_postgres::Statement>> {
        Ok(match self.mode {
            PostgresMode::Direct => Some(connection.prepare_cached(sql).await?),
            PostgresMode::PgBouncer => None,
        })
    }

    pub(crate) async fn fetch<M>(&self, sql: String, params: Vec<DinocoValue>, decode: Decode) -> anyhow::Result<Vec<M>>
    where
        M: DinocoPostgres,
    {
        let connection = self.connection().await?;
        let rows = match self.statement(&connection, &sql).await? {
            Some(statement) => connection.query_raw(&statement, params.iter()).await?,
            None => connection.query_raw(sql.as_str(), params.iter()).await?,
        };

        decode_stream(rows, decode, M::from_deadpool_posgres_row, std::any::type_name::<M>()).await
    }

    pub(crate) async fn run(&self, sql: String, params: Vec<DinocoValue>) -> anyhow::Result<usize> {
        let connection = self.connection().await?;
        let affected = match self.statement(&connection, &sql).await? {
            Some(statement) => connection.execute_raw(&statement, params.iter()).await?,
            None => connection.execute_raw(sql.as_str(), params.iter()).await?,
        };

        Ok(affected as usize)
    }

    pub(crate) async fn fetch_count(&self, sql: String, params: Vec<DinocoValue>) -> anyhow::Result<i64> {
        self.fetch_scalar(sql, params).await
    }

    pub(crate) async fn fetch_exists(&self, sql: String, params: Vec<DinocoValue>) -> anyhow::Result<bool> {
        self.fetch_scalar(sql, params).await
    }

    async fn fetch_scalar<T>(&self, sql: String, params: Vec<DinocoValue>) -> anyhow::Result<T>
    where
        T: for<'a> tokio_postgres::types::FromSql<'a>,
    {
        let connection = self.connection().await?;
        let params = param_refs(&params);
        let row = match self.statement(&connection, &sql).await? {
            Some(statement) => connection.query_one(&statement, &params).await?,
            None => connection.query_one(sql.as_str(), &params).await?,
        };

        Ok(row.try_get(0)?)
    }

    pub(crate) async fn begin_live_transaction(
        &self,
    ) -> anyhow::Result<tokio::sync::mpsc::Sender<LiveTransactionMessage>> {
        let pool = self.pool.clone();
        let mode = self.mode;
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
                        let result = match (*command).compile(Dialect::Postgres) {
                            Ok(command) => execute_transaction_command(&transaction, mode, &command)
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
        self.fetch_count(query.to_owned(), params.to_vec()).await
    }

    pub async fn query_exists(&self, query: &str, params: &[DinocoValue]) -> anyhow::Result<bool> {
        self.fetch_exists(query.to_owned(), params.to_vec()).await
    }
}

/// Decodes rows as the server streams them, so the raw rows are never all
/// held at once next to the decoded models.
async fn decode_stream<M>(
    rows: RowStream,
    decode: Decode,
    from_row: fn(&tokio_postgres::Row) -> Option<M>,
    model: &'static str,
) -> anyhow::Result<Vec<M>> {
    let mut rows = std::pin::pin!(rows);
    let mut items = Vec::new();

    while let Some(row) = rows.try_next().await? {
        match from_row(&row) {
            Some(item) => items.push(item),
            None if decode == Decode::Strict => return Err(RowDecodeError::new(model).into()),
            None => {}
        }
    }

    Ok(items)
}

async fn execute_transaction_command(
    transaction: &deadpool_postgres::Transaction<'_>,
    mode: PostgresMode,
    command: &CompiledTransactionCommand,
) -> anyhow::Result<RawTransactionOutput> {
    let mut output = None;
    for statement in &command.statements {
        let raw = execute_transaction_statement(transaction, mode, statement).await?;
        if statement.output {
            output = Some(raw);
        }
    }

    output.ok_or_else(|| anyhow!("Dinoco transaction command contains no output statement."))
}

async fn execute_transaction_statement(
    transaction: &deadpool_postgres::Transaction<'_>,
    mode: PostgresMode,
    command: &CompiledTransactionStatement,
) -> anyhow::Result<RawTransactionOutput> {
    if command.sql.is_empty() {
        return Ok(command.empty_output());
    }

    let params = param_refs(&command.params);
    let statement = match mode {
        PostgresMode::Direct => Some(transaction.prepare_cached(&command.sql).await?),
        PostgresMode::PgBouncer => None,
    };

    match command.kind {
        TransactionCommandKind::Rows => {
            let decoder = command
                .decoder
                .ok_or_else(|| anyhow!("Dinoco transaction query is missing its postgres row decoder."))?;
            let rows = match &statement {
                Some(statement) => transaction.query(statement, &params).await?,
                None => transaction.query(command.sql.as_str(), &params).await?,
            };
            Ok(RawTransactionOutput::Rows((decoder.postgres)(&rows)?))
        }
        TransactionCommandKind::Execute => {
            let affected = match &statement {
                Some(statement) => transaction.execute(statement, &params).await?,
                None => transaction.execute(command.sql.as_str(), &params).await?,
            };
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

    pub async fn query_count(&self, query: &str, params: &[DinocoValue]) -> anyhow::Result<i64> {
        self.inner.query_count(query, params).await
    }

    pub async fn query_exists(&self, query: &str, params: &[DinocoValue]) -> anyhow::Result<bool> {
        self.inner.query_exists(query, params).await
    }
}

fn param_refs(params: &[DinocoValue]) -> Vec<&(dyn ToSql + Sync)> {
    params.iter().map(|param| param as &(dyn ToSql + Sync)).collect()
}

type ToSqlResult = Result<IsNull, Box<dyn std::error::Error + Sync + Send>>;

/// Binds every value as the Rust type it holds, checked against the
/// parameter's PostgreSQL type the same way that type's own `ToSql` checks it,
/// so parameters are bound in place instead of boxed copies.
/// `NULL` binds to any type. An enum binds to a native enum of the same name
/// or to a text column; a `DateTime` binds to `TIMESTAMP` (as its UTC wall
/// clock time) and `TIMESTAMPTZ`.
impl ToSql for DinocoValue {
    fn to_sql(&self, ty: &Type, out: &mut tokio_postgres::types::private::BytesMut) -> ToSqlResult {
        match self {
            DinocoValue::Null => Ok(IsNull::Yes),
            DinocoValue::Integer(value) => value.to_sql(ty, out),
            DinocoValue::Float(value) => value.to_sql(ty, out),
            DinocoValue::String(value) => value.to_sql(ty, out),
            DinocoValue::Enum(name, value) => {
                if matches!(ty.kind(), Kind::Enum(_)) && !ty.name().eq_ignore_ascii_case(name) {
                    return Err(format!("enum `{name}` cannot be written to PostgreSQL enum `{}`", ty.name()).into());
                }
                value.as_str().to_sql(ty, out)
            }
            DinocoValue::Boolean(value) => value.to_sql(ty, out),
            DinocoValue::Bytes(value) => value.to_sql(ty, out),
            DinocoValue::Json(value) => Json(value).to_sql(ty, out),
            DinocoValue::DateTime(value) if *ty == Type::TIMESTAMP => value.naive_utc().to_sql(ty, out),
            DinocoValue::DateTime(value) if *ty == Type::TIMESTAMPTZ => value.to_sql(ty, out),
            DinocoValue::DateTime(_) => {
                Err(format!("DateTime<Utc> cannot be written to PostgreSQL type `{}`", ty.name()).into())
            }
            DinocoValue::Date(value) => value.to_sql(ty, out),
        }
    }

    fn accepts(_ty: &Type) -> bool {
        true
    }

    fn to_sql_checked(&self, ty: &Type, out: &mut tokio_postgres::types::private::BytesMut) -> ToSqlResult {
        let accepted = match self {
            DinocoValue::Null => true,
            DinocoValue::Integer(_) => <i64 as ToSql>::accepts(ty),
            DinocoValue::Float(_) => <f64 as ToSql>::accepts(ty),
            DinocoValue::String(_) => <String as ToSql>::accepts(ty),
            DinocoValue::Enum(..) => matches!(ty.kind(), Kind::Enum(_)) || <String as ToSql>::accepts(ty),
            DinocoValue::Boolean(_) => <bool as ToSql>::accepts(ty),
            DinocoValue::Bytes(_) => <Vec<u8> as ToSql>::accepts(ty),
            DinocoValue::Json(_) => <Json<&serde_json::Value> as ToSql>::accepts(ty),
            DinocoValue::DateTime(_) => *ty == Type::TIMESTAMP || *ty == Type::TIMESTAMPTZ,
            DinocoValue::Date(_) => <chrono::NaiveDate as ToSql>::accepts(ty),
        };

        if !accepted {
            return Err(Box::new(WrongType::new::<DinocoValue>(ty.clone())));
        }

        self.to_sql(ty, out)
    }
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
