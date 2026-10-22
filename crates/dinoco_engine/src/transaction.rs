use std::any::{Any, type_name};

use tokio::sync::{mpsc, oneshot};

#[cfg(any(feature = "postgres", feature = "mysql"))]
use crate::RowDecodeError;
use crate::backends::sql::{self, Dialect};
use crate::{
    AggregateQuery, CountQuery, DeleteQuery, DinocoRowModel, DinocoValue, ExistsQuery, FindQuery, InsertQuery,
    LookupQuery, ManyToManyRelationCountQuery, ManyToManyRelationQuery, OrderByList, PluckValue, RelationCountQuery,
    RelationOccurrenceQuery, UpdateQuery,
};
#[cfg(feature = "mysql")]
use crate::{DinocoMysql, MysqlRow};
#[cfg(feature = "postgres")]
use crate::{DinocoPostgres, PostgresRow};

pub(crate) type TransactionAny = Box<dyn Any + Send>;

/// A compiled statement and its bound values.
#[cfg(feature = "mysql")]
type Compiled = (String, Vec<DinocoValue>);

pub(crate) struct TransactionValue {
    value: TransactionAny,
    type_name: &'static str,
}

impl TransactionValue {
    fn into_typed<T>(self) -> anyhow::Result<T>
    where
        T: Send + 'static,
    {
        let actual_type = self.type_name;
        self.value
            .downcast::<T>()
            .map(|value| *value)
            .map_err(|_| anyhow::anyhow!("Transaction result has type `{actual_type}`, not `{}`.", type_name::<T>(),))
    }
}

pub(crate) enum LiveTransactionMessage {
    /// The command is boxed: the channel's buffer is sized by its largest
    /// message.
    Execute {
        command: Box<TransactionCommand>,
        reply: oneshot::Sender<anyhow::Result<TransactionValue>>,
    },
    Commit {
        reply: oneshot::Sender<anyhow::Result<()>>,
    },
    Rollback {
        reply: oneshot::Sender<anyhow::Result<()>>,
    },
}

/// A live transaction pinned to one physical database connection.
#[derive(Clone)]
pub struct TransactionExecutor {
    pub(crate) sender: mpsc::Sender<LiveTransactionMessage>,
    pub(crate) mysql: bool,
}

impl TransactionExecutor {
    pub fn is_mysql(&self) -> bool {
        self.mysql
    }

    pub async fn execute<T>(&self, command: TransactionCommand) -> anyhow::Result<T>
    where
        T: Send + 'static,
    {
        // Waiting for a free slot before building the message keeps the
        // command out of the wait's state.
        let permit = self
            .sender
            .reserve()
            .await
            .map_err(|_| anyhow::anyhow!("database transaction worker stopped unexpectedly"))?;
        let (reply, result) = oneshot::channel();
        permit.send(LiveTransactionMessage::Execute { command: Box::new(command), reply });
        result
            .await
            .map_err(|_| anyhow::anyhow!("database transaction worker dropped its operation result"))??
            .into_typed()
    }

    pub async fn commit(self) -> anyhow::Result<()> {
        self.finish(true).await
    }

    pub async fn rollback(self) -> anyhow::Result<()> {
        self.finish(false).await
    }

    async fn finish(self, commit: bool) -> anyhow::Result<()> {
        let (reply, result) = oneshot::channel();
        let message =
            if commit { LiveTransactionMessage::Commit { reply } } else { LiveTransactionMessage::Rollback { reply } };
        self.sender
            .send(message)
            .await
            .map_err(|_| anyhow::anyhow!("database transaction worker stopped before finalization"))?;
        result.await.map_err(|_| anyhow::anyhow!("database transaction worker dropped its finalization result"))?
    }
}

pub struct TransactionCommand {
    statement: TransactionStatement,
    output: TransactionOutput,
}

enum TransactionStatement {
    Find(FindQuery),
    Aggregate(AggregateQuery),
    Lookup(LookupQuery),
    RelationOccurrences(RelationOccurrenceQuery, Vec<DinocoValue>),
    ManyToManyRelation(ManyToManyRelationQuery, Vec<DinocoValue>),
    Insert(InsertQuery),
    Update(UpdateQuery),
    Delete(DeleteQuery),
    Count(CountQuery),
    Exists(ExistsQuery),
    RelationCount(RelationCountQuery),
    ManyToManyRelationCount(ManyToManyRelationCountQuery),
    MysqlTimeZoneCheck(chrono_tz::Tz),
}

#[derive(Clone, Copy)]
struct TransactionOutput {
    kind: TransactionCommandKind,
    decoder: Option<TransactionRowDecoder>,
    type_name: &'static str,
    atomic_update_returning: bool,
}

/// Decodes a statement's result into the boxed value its command returns:
/// every row into one `Vec<M>`, or the first column of the first row for a
/// scalar such as a count.
#[derive(Clone, Copy)]
pub(crate) struct TransactionRowDecoder {
    #[cfg(feature = "sqlite")]
    pub sqlite: fn(&mut rusqlite::Rows<'_>) -> anyhow::Result<DecodedRows>,
    #[cfg(feature = "postgres")]
    pub postgres: fn(&[PostgresRow]) -> anyhow::Result<DecodedRows>,
    #[cfg(feature = "mysql")]
    pub mysql: fn(Vec<MysqlRow>) -> anyhow::Result<DecodedRows>,
    pub empty: fn() -> DecodedRows,
}

/// The rows of one statement, as the boxed `Vec<M>` (or scalar) the command
/// returns.
pub(crate) struct DecodedRows {
    pub rows: TransactionAny,
    /// Read by the MySQL atomic update, which fails on an empty read-back.
    #[cfg_attr(not(feature = "mysql"), allow(dead_code))]
    pub len: usize,
}

impl DecodedRows {
    pub(crate) fn new<M>(rows: Vec<M>) -> Self
    where
        M: Send + 'static,
    {
        let len = rows.len();
        Self { rows: Box::new(rows), len }
    }

    pub(crate) fn scalar<T>(value: T) -> Self
    where
        T: Send + 'static,
    {
        Self { rows: Box::new(value), len: 1 }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum TransactionCommandKind {
    Rows,
    Execute,
}

pub(crate) enum RawTransactionOutput {
    /// Produced by the statement's [`TransactionRowDecoder`].
    Rows(DecodedRows),
    Affected(usize),
}

pub(crate) struct CompiledTransactionCommand {
    pub statements: Vec<CompiledTransactionStatement>,
    kind: TransactionCommandKind,
    type_name: &'static str,
    #[cfg_attr(not(feature = "mysql"), allow(dead_code))]
    pub atomic_update_returning: bool,
}

pub(crate) struct CompiledTransactionStatement {
    pub sql: String,
    pub params: Vec<DinocoValue>,
    pub kind: TransactionCommandKind,
    pub decoder: Option<TransactionRowDecoder>,
    pub output: bool,
}

impl CompiledTransactionStatement {
    /// The result of a statement that compiled to no SQL at all.
    pub(crate) fn empty_output(&self) -> RawTransactionOutput {
        match (self.kind, self.decoder) {
            (TransactionCommandKind::Rows, Some(decoder)) => RawTransactionOutput::Rows((decoder.empty)()),
            _ => RawTransactionOutput::Affected(0),
        }
    }
}

impl TransactionCommand {
    pub fn find_many<M>(query: FindQuery) -> Self
    where
        M: DinocoRowModel,
    {
        Self::rows::<M>(TransactionStatement::Find(query))
    }

    /// One row of `M` for the whole aggregate, or one per group.
    pub fn aggregate<M>(query: AggregateQuery) -> Self
    where
        M: DinocoRowModel,
    {
        Self::rows::<M>(TransactionStatement::Aggregate(query))
    }

    /// The row each key tuple names, in the order of the keys.
    pub fn lookup<M>(query: LookupQuery) -> Self
    where
        M: DinocoRowModel,
    {
        Self::rows::<M>(TransactionStatement::Lookup(query))
    }

    /// Loads relation rows for `includes(...)`; `params` are the parent keys
    /// placed before the query's own parameters.
    #[doc(hidden)]
    pub fn relation_occurrences<M>(query: RelationOccurrenceQuery, params: Vec<DinocoValue>) -> Self
    where
        M: DinocoRowModel,
    {
        Self::rows::<M>(TransactionStatement::RelationOccurrences(query, params))
    }

    #[doc(hidden)]
    pub fn many_to_many_relation<M>(query: ManyToManyRelationQuery, params: Vec<DinocoValue>) -> Self
    where
        M: DinocoRowModel,
    {
        Self::rows::<M>(TransactionStatement::ManyToManyRelation(query, params))
    }

    pub fn insert(query: InsertQuery) -> Self {
        Self::unit(TransactionStatement::Insert(query))
    }

    pub fn insert_returning_many<M>(query: InsertQuery) -> Self
    where
        M: DinocoRowModel,
    {
        Self::rows::<M>(TransactionStatement::Insert(query))
    }

    pub fn update(query: UpdateQuery) -> Self {
        Self::unit(TransactionStatement::Update(query))
    }

    pub fn update_returning<M>(query: UpdateQuery) -> Self
    where
        M: DinocoRowModel,
    {
        Self::rows::<M>(TransactionStatement::Update(query))
    }

    pub fn delete(query: DeleteQuery) -> Self {
        Self::unit(TransactionStatement::Delete(query))
    }

    pub fn delete_returning<M>(query: DeleteQuery) -> Self
    where
        M: DinocoRowModel,
    {
        Self::rows::<M>(TransactionStatement::Delete(query))
    }

    pub fn atomic_update_returning<M>(query: UpdateQuery) -> Self
    where
        M: DinocoRowModel,
    {
        let mut command = Self::update_returning::<M>(query);
        command.output.atomic_update_returning = true;
        command
    }

    /// MySQL only: `1` when the server can compute dates in `zone` (see
    /// `Backend::time_zone_supported`); executes to an `i64`.
    #[doc(hidden)]
    pub fn mysql_time_zone_check(zone: chrono_tz::Tz) -> Self {
        Self::scalar::<i64>(TransactionStatement::MysqlTimeZoneCheck(zone))
    }

    /// Counts the matching rows; executes to an `i64`.
    pub fn count(query: CountQuery) -> Self {
        Self::scalar::<i64>(TransactionStatement::Count(query))
    }

    /// Checks whether any row matches; executes to a `bool`.
    pub fn exists(query: ExistsQuery) -> Self {
        Self::scalar::<bool>(TransactionStatement::Exists(query))
    }

    /// Counts the related rows of `count(...).includes(...)`; executes to an
    /// `i64`.
    pub fn relation_count(query: RelationCountQuery) -> Self {
        Self::scalar::<i64>(TransactionStatement::RelationCount(query))
    }

    /// [`relation_count`](Self::relation_count) through a join table.
    pub fn many_to_many_relation_count(query: ManyToManyRelationCountQuery) -> Self {
        Self::scalar::<i64>(TransactionStatement::ManyToManyRelationCount(query))
    }

    pub(crate) fn compile(self, dialect: Dialect) -> anyhow::Result<CompiledTransactionCommand> {
        let output = self.output;
        #[cfg(feature = "mysql")]
        if output.atomic_update_returning && dialect == Dialect::Mysql {
            return self.compile_mysql_atomic_update();
        }

        let (sql, params) = match self.statement {
            TransactionStatement::Find(query) => sql::find(dialect, query),
            TransactionStatement::Aggregate(query) => sql::aggregate(dialect, query),
            TransactionStatement::Lookup(query) => sql::lookup(dialect, query),
            TransactionStatement::RelationOccurrences(query, keys) => {
                sql::relation_occurrence_with_keys(dialect, query, keys)
            }
            TransactionStatement::ManyToManyRelation(query, keys) => {
                sql::many_to_many_relation_with_keys(dialect, query, keys)
            }
            TransactionStatement::Insert(query) => sql::insert(dialect, query),
            TransactionStatement::Update(query) => sql::update(dialect, query),
            TransactionStatement::Delete(query) => sql::delete(dialect, query),
            TransactionStatement::Count(query) => sql::count(dialect, query),
            TransactionStatement::Exists(query) => sql::exists(dialect, query),
            TransactionStatement::RelationCount(query) => sql::relation_count(dialect, query),
            TransactionStatement::ManyToManyRelationCount(query) => sql::many_to_many_relation_count(dialect, query),
            TransactionStatement::MysqlTimeZoneCheck(zone) => sql::mysql_time_zone_check(zone),
        };

        Ok(CompiledTransactionCommand {
            statements: vec![CompiledTransactionStatement {
                sql,
                params,
                kind: output.kind,
                decoder: output.decoder,
                output: true,
            }],
            kind: output.kind,
            type_name: output.type_name,
            atomic_update_returning: output.atomic_update_returning,
        })
    }

    #[cfg(feature = "mysql")]
    fn compile_mysql_atomic_update(self) -> anyhow::Result<CompiledTransactionCommand> {
        let TransactionStatement::Update(update) = self.statement else {
            anyhow::bail!("MySQL atomic update returning received a non-update statement");
        };
        let ((update_sql, update_params), (find_sql, find_params)) = mysql_atomic_update_statements(update)?;

        Ok(CompiledTransactionCommand {
            statements: vec![
                CompiledTransactionStatement {
                    sql: update_sql,
                    params: update_params,
                    kind: TransactionCommandKind::Execute,
                    decoder: None,
                    output: false,
                },
                CompiledTransactionStatement {
                    sql: find_sql,
                    params: find_params,
                    kind: TransactionCommandKind::Rows,
                    decoder: self.output.decoder,
                    output: true,
                },
            ],
            kind: self.output.kind,
            type_name: self.output.type_name,
            atomic_update_returning: true,
        })
    }

    fn unit(statement: TransactionStatement) -> Self {
        Self {
            statement,
            output: TransactionOutput {
                kind: TransactionCommandKind::Execute,
                decoder: None,
                type_name: type_name::<usize>(),
                atomic_update_returning: false,
            },
        }
    }

    fn rows<M>(statement: TransactionStatement) -> Self
    where
        M: DinocoRowModel,
    {
        Self {
            statement,
            output: TransactionOutput {
                kind: TransactionCommandKind::Rows,
                decoder: Some(TransactionRowDecoder {
                    #[cfg(feature = "sqlite")]
                    sqlite: crate::backends::decode_sqlite_transaction_rows::<M>,
                    #[cfg(feature = "postgres")]
                    postgres: decode_postgres_rows::<M>,
                    #[cfg(feature = "mysql")]
                    mysql: decode_mysql_rows::<M>,
                    empty: empty_rows::<M>,
                }),
                type_name: type_name::<Vec<M>>(),
                atomic_update_returning: false,
            },
        }
    }

    /// A single-row, single-column statement whose command returns that
    /// column as a `T`, without a `Vec` around it.
    fn scalar<T>(statement: TransactionStatement) -> Self
    where
        T: Default + Send + 'static,
        PluckValue<T>: DinocoRowModel,
    {
        Self {
            statement,
            output: TransactionOutput {
                kind: TransactionCommandKind::Rows,
                decoder: Some(TransactionRowDecoder {
                    #[cfg(feature = "sqlite")]
                    sqlite: crate::backends::decode_sqlite_transaction_scalar::<T>,
                    #[cfg(feature = "postgres")]
                    postgres: decode_postgres_scalar::<T>,
                    #[cfg(feature = "mysql")]
                    mysql: decode_mysql_scalar::<T>,
                    empty: empty_scalar::<T>,
                }),
                type_name: type_name::<T>(),
                atomic_update_returning: false,
            },
        }
    }
}

#[cfg(feature = "mysql")]
/// The `UPDATE` and the read-back `SELECT` of an atomic update on MySQL, which
/// has no `RETURNING`. The row is found again by its `id` or by the values
/// the update set (see [`UpdateQuery::post_update_reload_conditions`]).
pub(crate) fn mysql_atomic_update_statements(mut update: UpdateQuery) -> anyhow::Result<(Compiled, Compiled)> {
    let returning =
        update.returning.ok_or_else(|| anyhow::anyhow!("MySQL atomic update returning requires a projection"))?;
    if update.sets.iter().any(|set| set.field == "id") {
        anyhow::bail!("MySQL atomic find_and_update cannot change the `id` field");
    }
    let find_conditions = update.post_update_reload_conditions();
    let table = update.table;
    update.returning = None;

    Ok((
        sql::update(Dialect::Mysql, update),
        sql::find(
            Dialect::Mysql,
            FindQuery {
                fields: returning,
                from: table,
                conditions: find_conditions,
                limit: 1,
                skip: -1,
                order_by: OrderByList::None,
            },
        ),
    ))
}

impl CompiledTransactionCommand {
    pub(crate) fn finish(self, raw: RawTransactionOutput) -> anyhow::Result<TransactionValue> {
        let value = match (self.kind, raw) {
            (TransactionCommandKind::Rows, RawTransactionOutput::Rows(decoded)) => decoded.rows,
            (TransactionCommandKind::Execute, RawTransactionOutput::Affected(affected)) => Box::new(affected),
            (TransactionCommandKind::Rows, _) => {
                anyhow::bail!("Dinoco transaction query received an unexpected database result.")
            }
            (TransactionCommandKind::Execute, _) => {
                anyhow::bail!("Dinoco transaction write received an unexpected database result.")
            }
        };

        Ok(TransactionValue { value, type_name: self.type_name })
    }
}

fn empty_rows<M>() -> DecodedRows
where
    M: Send + 'static,
{
    DecodedRows::new(Vec::<M>::new())
}

fn empty_scalar<T>() -> DecodedRows
where
    T: Default + Send + 'static,
{
    DecodedRows::scalar(T::default())
}

/// The error of a scalar statement whose result has no row to read.
pub(crate) fn missing_scalar_row() -> anyhow::Error {
    anyhow::anyhow!("Dinoco transaction aggregate returned no row.")
}

#[cfg(feature = "postgres")]
fn decode_postgres_rows<M>(rows: &[PostgresRow]) -> anyhow::Result<DecodedRows>
where
    M: DinocoPostgres,
{
    let rows = rows
        .iter()
        .map(|row| M::from_postgres_row(row).ok_or_else(|| RowDecodeError::new(type_name::<M>())))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(DecodedRows::new(rows))
}

#[cfg(feature = "mysql")]
fn decode_mysql_rows<M>(rows: Vec<MysqlRow>) -> anyhow::Result<DecodedRows>
where
    M: DinocoMysql,
{
    let rows = rows
        .into_iter()
        .map(|row| M::from_mysql_row_owned(row).ok_or_else(|| RowDecodeError::new(type_name::<M>())))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(DecodedRows::new(rows))
}

#[cfg(feature = "postgres")]
fn decode_postgres_scalar<T>(rows: &[PostgresRow]) -> anyhow::Result<DecodedRows>
where
    T: Send + 'static,
    PluckValue<T>: DinocoPostgres,
{
    let row = rows.first().ok_or_else(missing_scalar_row)?;
    let PluckValue(value) =
        PluckValue::<T>::from_postgres_row(row).ok_or_else(|| RowDecodeError::new(type_name::<T>()))?;
    Ok(DecodedRows::scalar(value))
}

#[cfg(feature = "mysql")]
fn decode_mysql_scalar<T>(rows: Vec<MysqlRow>) -> anyhow::Result<DecodedRows>
where
    T: Send + 'static,
    PluckValue<T>: DinocoMysql,
{
    let row = rows.into_iter().next().ok_or_else(missing_scalar_row)?;
    let PluckValue(value) =
        PluckValue::<T>::from_mysql_row_owned(row).ok_or_else(|| RowDecodeError::new(type_name::<T>()))?;
    Ok(DecodedRows::scalar(value))
}
