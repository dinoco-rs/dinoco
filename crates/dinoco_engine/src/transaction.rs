use std::any::{Any, type_name};

use tokio::sync::{mpsc, oneshot};

use crate::{
    CountQuery, DeleteQuery, DinocoMysql, DinocoPostgres, DinocoRowModel, DinocoSqlCompiler, DinocoSqlite, DinocoValue,
    FindQuery, InsertQuery, ManyToManyWriteQuery, MysqlRow, PostgresRow, SqliteRow, UpdateQuery,
};

type TransactionAny = Box<dyn Any + Send>;
type TransactionFinish = Box<dyn FnOnce(RawTransactionOutput) -> anyhow::Result<TransactionAny> + Send + Sync>;

pub struct TransactionResults {
    values: Vec<Option<TransactionValue>>,
}

impl std::fmt::Debug for TransactionResults {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let types = self
            .values
            .iter()
            .map(|value| value.as_ref().map(|value| value.type_name).unwrap_or("<taken>"))
            .collect::<Vec<_>>();
        formatter.debug_struct("TransactionResults").field("types", &types).finish()
    }
}

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
    Execute { command: TransactionCommand, reply: oneshot::Sender<anyhow::Result<TransactionValue>> },
    Commit { reply: oneshot::Sender<anyhow::Result<()>> },
    Rollback { reply: oneshot::Sender<anyhow::Result<()>> },
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
        let (reply, result) = oneshot::channel();
        self.sender
            .send(LiveTransactionMessage::Execute { command, reply })
            .await
            .map_err(|_| anyhow::anyhow!("database transaction worker stopped unexpectedly"))?;
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

impl TransactionResults {
    pub(crate) fn new(values: Vec<TransactionValue>) -> Self {
        Self { values: values.into_iter().map(Some).collect() }
    }

    pub fn len(&self) -> usize {
        self.values.len()
    }

    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    pub fn get<T>(&self, index: usize) -> anyhow::Result<&T>
    where
        T: Send + 'static,
    {
        let result = self
            .values
            .get(index)
            .ok_or_else(|| anyhow::anyhow!("Transaction result index {index} is out of bounds."))?
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Transaction result at index {index} was already taken."))?;

        result.value.downcast_ref::<T>().ok_or_else(|| {
            anyhow::anyhow!(
                "Transaction result at index {index} has type `{}`, not `{}`.",
                result.type_name,
                type_name::<T>(),
            )
        })
    }

    pub fn take<T>(&mut self, index: usize) -> anyhow::Result<T>
    where
        T: Send + 'static,
    {
        let result = self
            .values
            .get_mut(index)
            .ok_or_else(|| anyhow::anyhow!("Transaction result index {index} is out of bounds."))?
            .take()
            .ok_or_else(|| anyhow::anyhow!("Transaction result at index {index} was already taken."))?;
        let actual_type = result.type_name;

        result.value.downcast::<T>().map(|value| *value).map_err(|_| {
            anyhow::anyhow!(
                "Transaction result at index {index} has type `{actual_type}`, not `{}`.",
                type_name::<T>(),
            )
        })
    }
}

pub struct TransactionCommand {
    statements: Vec<TransactionStatement>,
    output_statement: usize,
    output: TransactionOutputAdapter,
}

enum TransactionStatement {
    Find(FindQuery),
    Insert(InsertQuery),
    Update(UpdateQuery),
    Delete(DeleteQuery),
    Count(CountQuery),
    ConnectManyToMany(ManyToManyWriteQuery),
    DisconnectManyToMany(ManyToManyWriteQuery),
    Noop,
    Invalid(String),
}

struct TransactionOutputAdapter {
    kind: TransactionCommandKind,
    decoder: Option<TransactionRowDecoder>,
    finish: TransactionFinish,
    type_name: &'static str,
    atomic_update_returning: bool,
}

#[derive(Clone, Copy)]
pub(crate) struct TransactionRowDecoder {
    pub sqlite: fn(&SqliteRow<'_>) -> Option<TransactionAny>,
    pub postgres: fn(&PostgresRow) -> Option<TransactionAny>,
    pub mysql: fn(&MysqlRow) -> Option<TransactionAny>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum TransactionCommandKind {
    Rows,
    Execute,
    Count,
}

pub(crate) enum RawTransactionOutput {
    Rows(Vec<TransactionAny>),
    Affected(usize),
    Count(i64),
}

pub(crate) struct CompiledTransactionCommand {
    pub statements: Vec<CompiledTransactionStatement>,
    finish: TransactionFinish,
    type_name: &'static str,
    pub atomic_update_returning: bool,
}

pub(crate) struct CompiledTransactionStatement {
    pub sql: String,
    pub params: Vec<DinocoValue>,
    pub kind: TransactionCommandKind,
    pub decoder: Option<TransactionRowDecoder>,
    pub output: bool,
}

impl TransactionCommand {
    pub fn find_first<M>(query: FindQuery) -> Self
    where
        M: DinocoRowModel,
    {
        Self::rows::<M, Option<M>, _>(TransactionStatement::Find(query), |mut rows| Ok(rows.drain(..).next()))
    }

    pub fn find_many<M>(query: FindQuery) -> Self
    where
        M: DinocoRowModel,
    {
        Self::rows::<M, Vec<M>, _>(TransactionStatement::Find(query), Ok)
    }

    pub fn find_one<M>(query: FindQuery, missing_message: String) -> Self
    where
        M: DinocoRowModel,
    {
        Self::rows::<M, M, _>(TransactionStatement::Find(query), move |mut rows| {
            rows.pop().ok_or_else(|| anyhow::anyhow!(missing_message))
        })
    }

    pub fn insert(query: InsertQuery) -> Self {
        Self::unit(TransactionStatement::Insert(query))
    }

    pub fn empty_write() -> Self {
        Self::unit(TransactionStatement::Noop)
    }

    pub fn insert_returning<M>(query: InsertQuery, missing_message: String) -> Self
    where
        M: DinocoRowModel,
    {
        Self::rows::<M, M, _>(TransactionStatement::Insert(query), move |mut rows| {
            rows.drain(..).next().ok_or_else(|| anyhow::anyhow!(missing_message))
        })
    }

    pub fn insert_returning_many<M>(query: InsertQuery) -> Self
    where
        M: DinocoRowModel,
    {
        Self::rows::<M, Vec<M>, _>(TransactionStatement::Insert(query), Ok)
    }

    pub fn empty_rows<M>() -> Self
    where
        M: DinocoRowModel,
    {
        Self::rows::<M, Vec<M>, _>(TransactionStatement::Noop, Ok)
    }

    pub fn update(query: UpdateQuery) -> Self {
        Self::unit(TransactionStatement::Update(query))
    }

    pub fn update_returning<M>(query: UpdateQuery) -> Self
    where
        M: DinocoRowModel,
    {
        Self::rows::<M, Vec<M>, _>(TransactionStatement::Update(query), Ok)
    }

    pub fn update_returning_one<M>(query: UpdateQuery, missing_message: String) -> Self
    where
        M: DinocoRowModel,
    {
        Self::rows::<M, M, _>(TransactionStatement::Update(query), move |mut rows| {
            rows.pop().ok_or_else(|| anyhow::anyhow!(missing_message))
        })
    }

    pub fn delete(query: DeleteQuery) -> Self {
        Self::unit(TransactionStatement::Delete(query))
    }

    pub fn delete_returning<M>(query: DeleteQuery) -> Self
    where
        M: DinocoRowModel,
    {
        Self::rows::<M, Vec<M>, _>(TransactionStatement::Delete(query), Ok)
    }

    pub fn count<T>(query: CountQuery, mapper: fn(i64) -> T) -> Self
    where
        T: Send + 'static,
    {
        let finish = Box::new(move |raw| {
            let RawTransactionOutput::Count(total) = raw else {
                anyhow::bail!("Dinoco transaction count received an unexpected database result.");
            };

            Ok(Box::new(mapper(total)) as TransactionAny)
        });

        Self {
            statements: vec![TransactionStatement::Count(query)],
            output_statement: 0,
            output: TransactionOutputAdapter {
                kind: TransactionCommandKind::Count,
                decoder: None,
                finish,
                type_name: type_name::<T>(),
                atomic_update_returning: false,
            },
        }
    }

    pub fn invalid(message: impl Into<String>) -> Self {
        Self {
            statements: vec![TransactionStatement::Invalid(message.into())],
            output_statement: 0,
            output: TransactionOutputAdapter {
                kind: TransactionCommandKind::Execute,
                decoder: None,
                finish: Box::new(|_| Ok(Box::new(()) as TransactionAny)),
                type_name: type_name::<()>(),
                atomic_update_returning: false,
            },
        }
    }

    pub fn with_many_to_many_writes(
        mut self,
        connects: Vec<ManyToManyWriteQuery>,
        disconnects: Vec<ManyToManyWriteQuery>,
    ) -> Self {
        let prefix_len = connects.len() + disconnects.len();
        let mut statements = Vec::with_capacity(prefix_len + self.statements.len());
        statements.extend(connects.into_iter().map(TransactionStatement::ConnectManyToMany));
        statements.extend(disconnects.into_iter().map(TransactionStatement::DisconnectManyToMany));
        statements.append(&mut self.statements);
        self.statements = statements;
        self.output_statement += prefix_len;
        self
    }

    pub fn with_appended_many_to_many_connects(mut self, connects: Vec<ManyToManyWriteQuery>) -> Self {
        self.statements.extend(connects.into_iter().map(TransactionStatement::ConnectManyToMany));
        self
    }

    pub(crate) fn has_returning_write(&self) -> bool {
        self.statements.iter().any(|statement| match statement {
            TransactionStatement::Insert(query) => query.returning.is_some(),
            TransactionStatement::Update(query) => query.returning.is_some(),
            TransactionStatement::Delete(query) => query.returning.is_some(),
            _ => false,
        })
    }

    pub(crate) fn compile<C>(self, compiler: &C) -> anyhow::Result<CompiledTransactionCommand>
    where
        C: DinocoSqlCompiler,
    {
        let mut statements = Vec::with_capacity(self.statements.len());

        for (index, statement) in self.statements.into_iter().enumerate() {
            let (sql, params) = match statement {
                TransactionStatement::Find(query) => compiler.compile_find_query(query),
                TransactionStatement::Insert(query) => compiler.compile_insert_query(query),
                TransactionStatement::Update(query) => compiler.compile_update_query(query),
                TransactionStatement::Delete(query) => compiler.compile_delete_query(query),
                TransactionStatement::Count(query) => compiler.compile_count_query(query),
                TransactionStatement::ConnectManyToMany(query) => compiler.compile_connect_many_to_many_query(query),
                TransactionStatement::DisconnectManyToMany(query) => {
                    compiler.compile_disconnect_many_to_many_query(query)
                }
                TransactionStatement::Noop => (String::new(), Vec::new()),
                TransactionStatement::Invalid(message) => anyhow::bail!(message),
            };
            let is_output = index == self.output_statement;
            statements.push(CompiledTransactionStatement {
                sql,
                params,
                kind: if is_output { self.output.kind } else { TransactionCommandKind::Execute },
                decoder: if is_output { self.output.decoder } else { None },
                output: is_output,
            });
        }

        Ok(CompiledTransactionCommand {
            statements,
            finish: self.output.finish,
            type_name: self.output.type_name,
            atomic_update_returning: self.output.atomic_update_returning,
        })
    }

    pub(crate) fn compile_mysql(self, compiler: &crate::MySqlAdapter) -> anyhow::Result<CompiledTransactionCommand> {
        if !self.output.atomic_update_returning {
            return self.compile(compiler);
        }

        let mut statements = self.statements;
        if statements.len() != 1 {
            anyhow::bail!("MySQL atomic update returning expects exactly one update statement");
        }
        let TransactionStatement::Update(mut update) = statements.remove(0) else {
            anyhow::bail!("MySQL atomic update returning received a non-update statement");
        };
        let returning =
            update.returning.ok_or_else(|| anyhow::anyhow!("MySQL atomic update returning requires a projection"))?;
        if update.sets.iter().any(|set| set.field == "id") {
            anyhow::bail!("MySQL atomic find_and_update cannot change the `id` field");
        }
        let find_conditions = update.post_update_reload_conditions();
        let table = update.table;
        update.returning = None;
        let (update_sql, update_params) = compiler.compile_update_query(update);
        let (find_sql, find_params) = compiler.compile_find_query(FindQuery {
            fields: returning,
            from: table,
            conditions: find_conditions,
            limit: 1,
            skip: -1,
            order_by: None,
        });

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
            finish: self.output.finish,
            type_name: self.output.type_name,
            atomic_update_returning: true,
        })
    }

    fn unit(statement: TransactionStatement) -> Self {
        let finish = Box::new(|raw| {
            let RawTransactionOutput::Affected(_affected) = raw else {
                anyhow::bail!("Dinoco transaction write received an unexpected database result.");
            };

            Ok(Box::new(()) as TransactionAny)
        });

        Self {
            statements: vec![statement],
            output_statement: 0,
            output: TransactionOutputAdapter {
                kind: TransactionCommandKind::Execute,
                decoder: None,
                finish,
                type_name: type_name::<()>(),
                atomic_update_returning: false,
            },
        }
    }

    fn rows<M, T, F>(statement: TransactionStatement, mapper: F) -> Self
    where
        M: DinocoRowModel,
        T: Send + 'static,
        F: FnOnce(Vec<M>) -> anyhow::Result<T> + Send + Sync + 'static,
    {
        let finish = Box::new(move |raw| {
            let RawTransactionOutput::Rows(rows) = raw else {
                anyhow::bail!("Dinoco transaction query received an unexpected database result.");
            };
            let rows = rows
                .into_iter()
                .map(|row| {
                    row.downcast::<M>().map(|row| *row).map_err(|_| {
                        anyhow::anyhow!("Dinoco could not decode a transaction row as `{}`.", type_name::<M>())
                    })
                })
                .collect::<anyhow::Result<Vec<_>>>()?;

            Ok(Box::new(mapper(rows)?) as TransactionAny)
        });

        Self {
            statements: vec![statement],
            output_statement: 0,
            output: TransactionOutputAdapter {
                kind: TransactionCommandKind::Rows,
                decoder: Some(TransactionRowDecoder {
                    sqlite: decode_sqlite_row::<M>,
                    postgres: decode_postgres_row::<M>,
                    mysql: decode_mysql_row::<M>,
                }),
                finish,
                type_name: type_name::<T>(),
                atomic_update_returning: false,
            },
        }
    }

    pub fn atomic_update_returning<M>(query: UpdateQuery) -> Self
    where
        M: DinocoRowModel,
    {
        let mut command = Self::update_returning::<M>(query);
        command.output.atomic_update_returning = true;
        command
    }
}

impl CompiledTransactionCommand {
    pub(crate) fn finish(self, raw: RawTransactionOutput) -> anyhow::Result<TransactionValue> {
        Ok(TransactionValue { value: (self.finish)(raw)?, type_name: self.type_name })
    }
}

fn decode_sqlite_row<M>(row: &SqliteRow<'_>) -> Option<TransactionAny>
where
    M: DinocoSqlite,
{
    M::from_sqlite_row(row).map(|row| Box::new(row) as TransactionAny)
}

fn decode_postgres_row<M>(row: &PostgresRow) -> Option<TransactionAny>
where
    M: DinocoPostgres,
{
    M::from_postgres_row(row).map(|row| Box::new(row) as TransactionAny)
}

fn decode_mysql_row<M>(row: &MysqlRow) -> Option<TransactionAny>
where
    M: DinocoMysql,
{
    M::from_mysql_row(row).map(|row| Box::new(row) as TransactionAny)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FindWhere, MySqlAdapter, SingleIdRow, UpdateOperation, UpdateSet};

    #[test]
    fn mysql_atomic_update_compiles_the_update_before_its_compatibility_read() {
        let command = TransactionCommand::atomic_update_returning::<SingleIdRow>(UpdateQuery {
            table: "business",
            sets: vec![UpdateSet {
                field: "balance",
                value: DinocoValue::Integer(80),
                operation: UpdateOperation::Decrement,
            }],
            conditions: vec![
                FindWhere::Eq("id", DinocoValue::String("business-1".to_string())),
                FindWhere::Gte("balance", DinocoValue::Integer(80)),
            ],
            returning: Some(&["id"]),
        });

        let compiled = command
            .compile_mysql(&MySqlAdapter::new("mysql://root:root@localhost/mysql"))
            .expect("compile atomic update");

        assert!(compiled.atomic_update_returning);
        assert_eq!(compiled.statements.len(), 2);
        assert_eq!(
            compiled.statements[0].sql,
            "UPDATE business SET balance = balance - ? WHERE id = ? AND balance >= ?"
        );
        assert!(matches!(compiled.statements[0].kind, TransactionCommandKind::Execute));
        assert_eq!(compiled.statements[1].sql, "SELECT id FROM business WHERE id = ? LIMIT ?");
        assert!(matches!(compiled.statements[1].kind, TransactionCommandKind::Rows));
    }

    #[test]
    fn mysql_atomic_update_reloads_a_changed_filter_by_its_new_set_value() {
        let command = TransactionCommand::atomic_update_returning::<SingleIdRow>(UpdateQuery {
            table: "document",
            sets: vec![UpdateSet {
                field: "body",
                value: DinocoValue::String("updated body".to_string()),
                operation: UpdateOperation::Set,
            }],
            conditions: vec![FindWhere::FullText(&["body"], DinocoValue::String("original".to_string()))],
            returning: Some(&["id"]),
        });

        let compiled = command
            .compile_mysql(&MySqlAdapter::new("mysql://root:root@localhost/mysql"))
            .expect("compile atomic update");

        assert_eq!(
            compiled.statements[0].sql,
            "UPDATE document SET body = ? WHERE MATCH (body) AGAINST (? IN NATURAL LANGUAGE MODE)"
        );
        assert_eq!(compiled.statements[1].sql, "SELECT id FROM document WHERE body = ? LIMIT ?");
        assert_eq!(compiled.statements[1].params[0], DinocoValue::String("updated body".to_string()));
    }
}
