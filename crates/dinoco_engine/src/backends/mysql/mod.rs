use std::sync::Arc;

use anyhow::{Context, anyhow};
use mysql_async::prelude::Queryable;
use mysql_async::{Opts, OptsBuilder, Pool, TxOpts, Value};

mod compiler;

use super::Decode;
use super::sql::Dialect;
use crate::{
    CompiledTransactionCommand, CompiledTransactionStatement, DinocoAdapter, DinocoMysql, DinocoRowModel, DinocoValue,
    LiveTransactionMessage, RawTransactionOutput, RowDecodeError, TransactionCommandKind,
};

#[derive(Clone)]
pub struct MySqlAdapter {
    pub url: String,
    pub pool: Arc<Pool>,
    with_logger: bool,
}

#[async_trait::async_trait]
impl DinocoAdapter for MySqlAdapter {
    async fn new(url: String) -> Result<Self, String> {
        Ok(Self::new(url))
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

impl MySqlAdapter {
    /// Creates the connection pool for `url`.
    ///
    /// Unless the URL sets `reset_connection`, pooled connections are not
    /// reset when they return to the pool: a reset drops the connection's
    /// prepared statements, which would cost every query an extra round trip
    /// to prepare its statement again. Dinoco does not leave session state
    /// behind (open transactions are rolled back by the driver), but raw SQL
    /// that changes session variables should opt back in with
    /// `?reset_connection=true`.
    pub fn new(url: impl Into<String>) -> Self {
        let url = url.into();
        let pool = match Opts::from_url(&url) {
            Ok(opts) if !url_sets_option(&url, "reset_connection") => {
                let pool_opts = opts.pool_opts().clone().with_reset_connection(false);
                Pool::new(OptsBuilder::from_opts(opts).pool_opts(pool_opts))
            }
            Ok(opts) => Pool::new(opts),
            // Reports the invalid URL exactly as the driver always has.
            Err(_) => Pool::new(url.as_str()),
        };

        Self { url, pool: Arc::new(pool), with_logger: false }
    }

    pub(crate) fn set_logger(&mut self, enabled: bool) {
        self.with_logger = enabled;
    }

    pub(crate) fn logger_enabled(&self) -> bool {
        self.with_logger
    }

    async fn connection(&self) -> anyhow::Result<mysql_async::Conn> {
        self.pool.get_conn().await.context("Failed to get mysql connection from pool")
    }

    pub(crate) async fn fetch<M>(&self, sql: String, params: Vec<DinocoValue>, decode: Decode) -> anyhow::Result<Vec<M>>
    where
        M: DinocoMysql,
    {
        let mut connection = self.connection().await?;
        fetch_rows(&mut connection, sql, params, decode).await
    }

    pub(crate) async fn run(&self, sql: String, params: Vec<DinocoValue>) -> anyhow::Result<usize> {
        let mut connection = self.connection().await?;
        connection.exec_drop(sql, mysql_params(params)).await?;

        Ok(connection.affected_rows() as usize)
    }

    pub(crate) async fn fetch_count(&self, sql: String, params: Vec<DinocoValue>) -> anyhow::Result<i64> {
        let mut connection = self.connection().await?;
        Ok(connection.exec_first::<i64, _, _>(sql, mysql_params(params)).await?.unwrap_or_default())
    }

    pub(crate) async fn fetch_exists(&self, sql: String, params: Vec<DinocoValue>) -> anyhow::Result<bool> {
        Ok(self.fetch_count(sql, params).await? != 0)
    }

    pub(crate) async fn begin_live_transaction(
        &self,
    ) -> anyhow::Result<tokio::sync::mpsc::Sender<LiveTransactionMessage>> {
        let pool = self.pool.clone();
        let (sender, mut receiver) = tokio::sync::mpsc::channel(1);
        let (ready_sender, ready_receiver) = tokio::sync::oneshot::channel();

        tokio::spawn(async move {
            let mut connection = match pool.get_conn().await {
                Ok(connection) => connection,
                Err(error) => {
                    let _ = ready_sender.send(Err(anyhow::Error::from(error)));
                    return;
                }
            };
            let mut transaction = match connection.start_transaction(TxOpts::default()).await {
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
                        let result = match (*command).compile(Dialect::Mysql) {
                            Ok(mut command) => execute_transaction_command(&mut transaction, &mut command)
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

        ready_receiver.await.map_err(|_| anyhow!("mysql transaction worker stopped during BEGIN"))??;
        Ok(sender)
    }

    /// Runs the `UPDATE` and the read-back `SELECT` of an atomic update in
    /// one transaction, so the row read is the row the update changed.
    pub(crate) async fn execute_atomic_update_returning<M>(
        &self,
        (update_sql, update_params): (String, Vec<DinocoValue>),
        (find_sql, find_params): (String, Vec<DinocoValue>),
    ) -> anyhow::Result<Vec<M>>
    where
        M: DinocoRowModel,
    {
        let mut connection = self.connection().await?;
        let mut transaction = connection.start_transaction(TxOpts::default()).await?;

        let execution = async {
            transaction.exec_drop(update_sql, mysql_params(update_params)).await?;
            if transaction.affected_rows() == 0 {
                return Ok(Vec::new());
            }

            let decoded = fetch_rows::<M, _>(&mut transaction, find_sql, find_params, Decode::Strict).await?;
            if decoded.is_empty() {
                anyhow::bail!(
                    "MySQL atomic update affected a row but it no longer matched the post-update reload conditions"
                );
            }
            Ok(decoded)
        }
        .await;

        match execution {
            Ok(rows) => {
                transaction.commit().await?;
                Ok(rows)
            }
            Err(source) => match transaction.rollback().await {
                Ok(()) => Err(source),
                Err(rollback_error) => {
                    Err(source.context(format!("MySQL atomic update rollback also failed: {rollback_error}")))
                }
            },
        }
    }

    pub async fn query_count(&self, query: &str, params: &[DinocoValue]) -> anyhow::Result<i64> {
        self.fetch_count(query.to_owned(), params.to_vec()).await
    }

    pub async fn query_exists(&self, query: &str, params: &[DinocoValue]) -> anyhow::Result<bool> {
        self.fetch_exists(query.to_owned(), params.to_vec()).await
    }
}

/// Whether the query string of `url` sets `option`.
fn url_sets_option(url: &str, option: &str) -> bool {
    url.split_once('?').is_some_and(|(_, query)| {
        query.split('&').any(|pair| pair.split_once('=').map_or(pair, |(key, _)| key) == option)
    })
}

/// Reads the rows of `sql` as they arrive and moves each one into a model,
/// without holding every raw row at once or copying column values.
async fn fetch_rows<M, Q>(
    queryable: &mut Q,
    sql: String,
    params: Vec<DinocoValue>,
    decode: Decode,
) -> anyhow::Result<Vec<M>>
where
    M: DinocoMysql,
    Q: Queryable,
{
    let mut result = queryable.exec_iter(sql, mysql_params(params)).await?;
    let mut items = Vec::new();
    let mut undecodable = false;

    result
        .for_each(|row| match M::from_mysql_row_owned(row) {
            Some(item) => items.push(item),
            None => undecodable = true,
        })
        .await?;
    drop(result);

    if undecodable && decode == Decode::Strict {
        return Err(RowDecodeError::new(std::any::type_name::<M>()).into());
    }

    Ok(items)
}

async fn execute_transaction_command(
    transaction: &mut mysql_async::Transaction<'_>,
    command: &mut CompiledTransactionCommand,
) -> anyhow::Result<RawTransactionOutput> {
    if command.atomic_update_returning {
        let affected = execute_transaction_statement(transaction, &mut command.statements[0]).await?;
        let RawTransactionOutput::Affected(affected) = affected else {
            anyhow::bail!("MySQL atomic update received an unexpected database result");
        };
        if affected == 0 {
            return Ok(command.statements[1].empty_output());
        }
        let rows = execute_transaction_statement(transaction, &mut command.statements[1]).await?;
        if matches!(&rows, RawTransactionOutput::Rows(rows) if rows.len == 0) {
            anyhow::bail!(
                "MySQL atomic update affected a row but it no longer matched the post-update reload conditions"
            );
        }
        return Ok(rows);
    }

    let mut output = None;
    for statement in &mut command.statements {
        let raw = execute_transaction_statement(transaction, statement).await?;
        if statement.output {
            output = Some(raw);
        }
    }

    output.ok_or_else(|| anyhow!("Dinoco transaction command contains no output statement."))
}

async fn execute_transaction_statement(
    transaction: &mut mysql_async::Transaction<'_>,
    command: &mut CompiledTransactionStatement,
) -> anyhow::Result<RawTransactionOutput> {
    if command.sql.is_empty() {
        return Ok(command.empty_output());
    }

    let params = mysql_params(std::mem::take(&mut command.params));
    match command.kind {
        TransactionCommandKind::Rows => {
            let decoder =
                command.decoder.ok_or_else(|| anyhow!("Dinoco transaction query is missing its mysql row decoder."))?;
            let rows = transaction.exec::<mysql_async::Row, _, _>(command.sql.as_str(), params).await?;
            Ok(RawTransactionOutput::Rows((decoder.mysql)(rows)?))
        }
        TransactionCommandKind::Execute => {
            transaction.exec_drop(command.sql.as_str(), params).await?;
            Ok(RawTransactionOutput::Affected(transaction.affected_rows() as usize))
        }
    }
}

fn mysql_params(params: Vec<DinocoValue>) -> Vec<Value> {
    params.into_iter().map(Value::from).collect()
}

impl mysql_common::prelude::FromValue for DinocoValue {
    type Intermediate = DinocoValue;
}

impl TryFrom<Value> for DinocoValue {
    type Error = mysql_common::value::convert::FromValueError;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        match value {
            Value::NULL => Ok(DinocoValue::Null),
            Value::Bytes(value) => String::from_utf8(value)
                .map(DinocoValue::String)
                .or_else(|error| Ok(DinocoValue::Bytes(error.into_bytes()))),
            Value::Int(value) => Ok(DinocoValue::Integer(value)),
            Value::UInt(value) => Ok(DinocoValue::Integer(value as i64)),
            Value::Float(value) => Ok(DinocoValue::Float(value as f64)),
            Value::Double(value) => Ok(DinocoValue::Float(value)),
            value => Err(mysql_common::value::convert::FromValueError(value)),
        }
    }
}

impl From<DinocoValue> for Value {
    fn from(value: DinocoValue) -> Self {
        match value {
            DinocoValue::Null => Value::NULL,
            DinocoValue::Integer(value) => Value::Int(value),
            DinocoValue::Float(value) => Value::Double(value),
            DinocoValue::String(value) | DinocoValue::Enum(_, value) => Value::Bytes(value.into_bytes()),
            DinocoValue::Boolean(value) => Value::Int(if value { 1 } else { 0 }),
            DinocoValue::Bytes(value) => Value::Bytes(value),
            DinocoValue::Json(value) => Value::Bytes(value.to_string().into_bytes()),
            DinocoValue::DateTime(value) => Value::Bytes(value.naive_utc().to_string().into_bytes()),
            DinocoValue::Date(value) => Value::Bytes(value.to_string().into_bytes()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::url_sets_option;

    #[test]
    fn finds_options_in_the_query_string_only() {
        assert!(url_sets_option("mysql://root@localhost/db?reset_connection=true", "reset_connection"));
        assert!(url_sets_option("mysql://root@localhost/db?pool_min=1&reset_connection=false", "reset_connection"));
        assert!(url_sets_option("mysql://root@localhost/db?reset_connection", "reset_connection"));
        assert!(!url_sets_option("mysql://root@localhost/db", "reset_connection"));
        assert!(!url_sets_option("mysql://root@localhost/reset_connection", "reset_connection"));
        assert!(!url_sets_option("mysql://root@localhost/db?no_reset_connection=1", "reset_connection"));
    }
}
