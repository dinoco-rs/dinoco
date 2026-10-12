use std::sync::Arc;

use anyhow::{Context, anyhow};
use deadpool_sqlite::{Config, Hook, HookError, Pool, Runtime};
use rusqlite::types::{FromSql, FromSqlError, FromSqlResult, ToSqlOutput, Value, ValueRef};

mod compiler;

use super::Decode;
use super::sql::Dialect;
use crate::{
    CompiledTransactionCommand, CompiledTransactionStatement, DinocoAdapter, DinocoSqlite, DinocoValue,
    LiveTransactionMessage, RawTransactionOutput, RowDecodeError, TransactionCommandKind,
};

#[derive(Clone)]
pub struct SqliteAdapter {
    pub path: String,
    pub pool: Arc<Pool>,
    with_logger: bool,
    /// Keeps an in-memory database alive while any clone of the adapter
    /// exists, even if the pool drops every connection.
    memory_anchor: Option<Arc<std::sync::Mutex<rusqlite::Connection>>>,
}

#[async_trait::async_trait]
impl DinocoAdapter for SqliteAdapter {
    async fn new(path: String) -> Result<Self, String> {
        let path = normalize_sqlite_path(path);
        if let Some(parent) = std::path::Path::new(&path).parent() {
            std::fs::create_dir_all(parent).map_err(|err| err.to_string())?;
        }
        let cfg = Config::new(&path);
        let pool = cfg
            .builder(Runtime::Tokio1)
            .map_err(|err| err.to_string())?
            .post_create(Hook::async_fn(|connection, _| {
                Box::pin(async move {
                    match connection
                        .interact(|conn| -> rusqlite::Result<bool> {
                            conn.pragma_update(None, "foreign_keys", true)?;
                            conn.pragma_query_value(None, "foreign_keys", |row| row.get::<_, bool>(0))
                        })
                        .await
                    {
                        Ok(Ok(true)) => Ok(()),
                        Ok(Ok(false)) => Err(HookError::message(
                            "SQLite did not enable foreign key enforcement for a new connection",
                        )),
                        Ok(Err(err)) => Err(HookError::Backend(err)),
                        Err(err) => Err(HookError::message(format!(
                            "failed to configure SQLite foreign key enforcement: {err}"
                        ))),
                    }
                })
            }))
            .build()
            .map_err(|err| err.to_string())?;

        // Open one pooled connection eagerly so `connect()` guarantees that a
        // file-backed SQLite database has been created and configured.
        let connection = pool.get().await.map_err(|err| err.to_string())?;
        drop(connection);

        Ok(Self { path, pool: Arc::new(pool), with_logger: false, memory_anchor: None })
    }

    async fn query<M>(&self, query: &str, params: &[DinocoValue]) -> anyhow::Result<Vec<M>>
    where
        M: DinocoSqlite,
    {
        self.fetch(query.to_owned(), params.to_vec(), Decode::Strict).await
    }

    async fn query_optional<M>(&self, query: &str, params: &[DinocoValue]) -> anyhow::Result<Vec<M>>
    where
        M: DinocoSqlite,
    {
        self.fetch(query.to_owned(), params.to_vec(), Decode::Lenient).await
    }

    async fn execute(&self, query: &str, params: &[DinocoValue]) -> anyhow::Result<usize> {
        self.run(query.to_owned(), params.to_vec()).await
    }
}

fn normalize_sqlite_path(path: String) -> String {
    if path == ":memory:"
        || std::path::Path::new(&path).is_absolute()
        || path.starts_with("file:")
        || path.starts_with("dinoco/")
    {
        path
    } else {
        format!("dinoco/{path}")
    }
}

impl SqliteAdapter {
    /// Opens a new, empty in-memory database that every pooled connection
    /// (and every transaction) of this adapter shares. Each call gets its own
    /// database, so parallel tests never see each other's rows. The database
    /// is dropped with the last clone of the adapter.
    pub async fn memory() -> anyhow::Result<Self> {
        static NEXT_DATABASE: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

        let id = NEXT_DATABASE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        // The memdb VFS shares one in-memory database between connections by
        // name and, unlike `cache=shared`, keeps SQLite's regular locking.
        let path = format!("file:/dinoco-memory-{}-{id}?vfs=memdb", std::process::id());
        let anchor = rusqlite::Connection::open(&path).context("failed to open the in-memory SQLite database")?;
        let mut adapter = <Self as DinocoAdapter>::new(path).await.map_err(anyhow::Error::msg)?;
        adapter.memory_anchor = Some(Arc::new(std::sync::Mutex::new(anchor)));

        Ok(adapter)
    }

    pub(crate) fn set_logger(&mut self, enabled: bool) {
        self.with_logger = enabled;
    }

    pub(crate) fn logger_enabled(&self) -> bool {
        self.with_logger
    }

    /// Runs `operation` on a pooled connection, on a blocking thread.
    async fn interact<T, F>(&self, operation: F) -> anyhow::Result<T>
    where
        F: FnOnce(&mut rusqlite::Connection) -> anyhow::Result<T> + Send + 'static,
        T: Send + 'static,
    {
        let connection = self.pool.get().await.context("Failed to get sqlite connection from pool")?;
        connection.interact(operation).await.map_err(|err| anyhow!(err.to_string()))?
    }

    pub(crate) async fn fetch<M>(&self, sql: String, params: Vec<DinocoValue>, decode: Decode) -> anyhow::Result<Vec<M>>
    where
        M: DinocoSqlite,
    {
        self.interact(move |connection| {
            let mut statement = connection.prepare_cached(&sql)?;
            let mut rows = statement.query(rusqlite::params_from_iter(&params))?;
            decode_rows(&mut rows, decode, std::any::type_name::<M>())
        })
        .await
    }

    pub(crate) async fn run(&self, sql: String, params: Vec<DinocoValue>) -> anyhow::Result<usize> {
        self.interact(move |connection| {
            let mut statement = connection.prepare_cached(&sql)?;
            Ok(statement.execute(rusqlite::params_from_iter(&params))?)
        })
        .await
    }

    pub(crate) async fn fetch_count(&self, sql: String, params: Vec<DinocoValue>) -> anyhow::Result<i64> {
        self.fetch_scalar(sql, params).await
    }

    pub(crate) async fn fetch_exists(&self, sql: String, params: Vec<DinocoValue>) -> anyhow::Result<bool> {
        self.fetch_scalar(sql, params).await
    }

    async fn fetch_scalar<T>(&self, sql: String, params: Vec<DinocoValue>) -> anyhow::Result<T>
    where
        T: FromSql + Send + 'static,
    {
        self.interact(move |connection| {
            let mut statement = connection.prepare_cached(&sql)?;
            Ok(statement.query_row(rusqlite::params_from_iter(&params), |row| row.get(0))?)
        })
        .await
    }

    pub(crate) async fn begin_live_transaction(
        &self,
    ) -> anyhow::Result<tokio::sync::mpsc::Sender<LiveTransactionMessage>> {
        let connection = self.pool.get().await.context("Failed to get sqlite connection from pool")?;
        let (sender, mut receiver) = tokio::sync::mpsc::channel(1);
        let (ready_sender, ready_receiver) = tokio::sync::oneshot::channel();

        tokio::spawn(async move {
            let _ = connection
                .interact(move |connection| -> anyhow::Result<()> {
                    let transaction = match connection.transaction() {
                        Ok(transaction) => transaction,
                        Err(error) => {
                            let _ = ready_sender.send(Err(anyhow::Error::from(error)));
                            return Ok(());
                        }
                    };
                    let _ = ready_sender.send(Ok(()));

                    while let Some(message) = receiver.blocking_recv() {
                        match message {
                            LiveTransactionMessage::Execute { command, reply } => {
                                let result = (*command).compile(Dialect::Sqlite).and_then(|command| {
                                    execute_transaction_command(&transaction, &command)
                                        .and_then(|raw| command.finish(raw))
                                });
                                let _ = reply.send(result);
                            }
                            LiveTransactionMessage::Commit { reply } => {
                                let result = transaction.commit().map_err(anyhow::Error::from);
                                let _ = reply.send(result);
                                return Ok(());
                            }
                            LiveTransactionMessage::Rollback { reply } => {
                                let result = transaction.rollback().map_err(anyhow::Error::from);
                                let _ = reply.send(result);
                                return Ok(());
                            }
                        }
                    }

                    // Dropping an unfinished rusqlite transaction rolls it back.
                    Ok(())
                })
                .await;
        });

        ready_receiver.await.map_err(|_| anyhow!("sqlite transaction worker stopped during BEGIN"))??;
        Ok(sender)
    }

    pub async fn query_count(&self, query: &str, params: &[DinocoValue]) -> anyhow::Result<i64> {
        self.fetch_count(query.to_owned(), params.to_vec()).await
    }

    pub async fn query_exists(&self, query: &str, params: &[DinocoValue]) -> anyhow::Result<bool> {
        self.fetch_exists(query.to_owned(), params.to_vec()).await
    }
}

fn decode_rows<M>(rows: &mut rusqlite::Rows<'_>, decode: Decode, model: &'static str) -> anyhow::Result<Vec<M>>
where
    M: DinocoSqlite,
{
    let mut items = Vec::new();

    while let Some(row) = rows.next()? {
        match M::from_sqlite_row(row) {
            Some(item) => items.push(item),
            None if decode == Decode::Strict => return Err(RowDecodeError::new(model).into()),
            None => {}
        }
    }

    Ok(items)
}

/// Decodes every row of a transaction query into a boxed `Vec<M>`.
pub(crate) fn decode_sqlite_transaction_rows<M>(rows: &mut rusqlite::Rows<'_>) -> anyhow::Result<crate::DecodedRows>
where
    M: DinocoSqlite,
{
    Ok(crate::DecodedRows::new(decode_rows::<M>(rows, Decode::Strict, std::any::type_name::<M>())?))
}

fn execute_transaction_command(
    transaction: &rusqlite::Transaction<'_>,
    command: &CompiledTransactionCommand,
) -> anyhow::Result<RawTransactionOutput> {
    let mut output = None;
    for statement in &command.statements {
        let raw = execute_transaction_statement(transaction, statement)?;
        if statement.output {
            output = Some(raw);
        }
    }

    output.ok_or_else(|| anyhow!("Dinoco transaction command contains no output statement."))
}

fn execute_transaction_statement(
    transaction: &rusqlite::Transaction<'_>,
    command: &CompiledTransactionStatement,
) -> anyhow::Result<RawTransactionOutput> {
    if command.sql.is_empty() {
        return Ok(command.empty_output());
    }

    let params = rusqlite::params_from_iter(&command.params);

    match command.kind {
        TransactionCommandKind::Rows => {
            let decoder = command
                .decoder
                .ok_or_else(|| anyhow!("Dinoco transaction query is missing its sqlite row decoder."))?;
            let mut statement = transaction.prepare_cached(&command.sql)?;
            let mut rows = statement.query(params)?;

            Ok(RawTransactionOutput::Rows((decoder.sqlite)(&mut rows)?))
        }
        TransactionCommandKind::Execute => {
            let mut statement = transaction.prepare_cached(&command.sql)?;
            Ok(RawTransactionOutput::Affected(statement.execute(params)?))
        }
    }
}

/// Text and blobs are bound by reference; SQLite copies them itself.
impl rusqlite::ToSql for DinocoValue {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(match self {
            DinocoValue::Null => ToSqlOutput::Borrowed(ValueRef::Null),
            DinocoValue::Integer(i) => ToSqlOutput::Owned(Value::Integer(*i)),
            DinocoValue::Float(f) => ToSqlOutput::Owned(Value::Real(*f)),
            DinocoValue::Boolean(b) => ToSqlOutput::Owned(Value::Integer(i64::from(*b))),
            DinocoValue::String(s) | DinocoValue::Enum(_, s) => ToSqlOutput::Borrowed(ValueRef::Text(s.as_bytes())),
            DinocoValue::Bytes(v) => ToSqlOutput::Borrowed(ValueRef::Blob(v)),
            DinocoValue::Json(v) => ToSqlOutput::Owned(Value::Blob(v.to_string().into_bytes())),
            DinocoValue::DateTime(dt) => ToSqlOutput::Owned(Value::Text(dt.to_rfc3339())),
            DinocoValue::Date(date) => ToSqlOutput::Owned(Value::Text(date.to_string())),
        })
    }
}

impl FromSql for DinocoValue {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        match value {
            ValueRef::Null => Ok(DinocoValue::Null),
            ValueRef::Integer(value) => Ok(DinocoValue::Integer(value)),
            ValueRef::Real(value) => Ok(DinocoValue::Float(value)),
            ValueRef::Text(value) => std::str::from_utf8(value)
                .map(|value| DinocoValue::String(value.to_owned()))
                .map_err(|err| FromSqlError::Other(Box::new(err))),
            ValueRef::Blob(value) => Ok(DinocoValue::Bytes(value.to_vec())),
        }
    }
}
