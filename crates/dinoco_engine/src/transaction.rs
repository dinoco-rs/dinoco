use std::any::{Any, type_name};

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

        Ok(CompiledTransactionCommand { statements, finish: self.output.finish, type_name: self.output.type_name })
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
            },
        }
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
