use std::any::{Any, type_name};

use crate::{
    CountQuery, DeleteQuery, DinocoMysql, DinocoPostgres, DinocoRowModel, DinocoSqlCompiler, DinocoSqlite, DinocoValue,
    FindQuery, InsertQuery, MysqlRow, PostgresRow, SqliteRow, UpdateQuery,
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
    statement: TransactionStatement,
    output: TransactionOutputAdapter,
}

enum TransactionStatement {
    Find(FindQuery),
    Insert(InsertQuery),
    Update(UpdateQuery),
    Delete(DeleteQuery),
    Count(CountQuery),
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
    pub sql: String,
    pub params: Vec<DinocoValue>,
    pub kind: TransactionCommandKind,
    pub decoder: Option<TransactionRowDecoder>,
    finish: TransactionFinish,
    type_name: &'static str,
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
            statement: TransactionStatement::Count(query),
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
            statement: TransactionStatement::Invalid(message.into()),
            output: TransactionOutputAdapter {
                kind: TransactionCommandKind::Execute,
                decoder: None,
                finish: Box::new(|_| Ok(Box::new(()) as TransactionAny)),
                type_name: type_name::<()>(),
            },
        }
    }

    pub(crate) fn has_returning_write(&self) -> bool {
        match &self.statement {
            TransactionStatement::Insert(query) => query.returning.is_some(),
            TransactionStatement::Update(query) => query.returning.is_some(),
            TransactionStatement::Delete(query) => query.returning.is_some(),
            _ => false,
        }
    }

    pub(crate) fn compile<C>(self, compiler: &C) -> anyhow::Result<CompiledTransactionCommand>
    where
        C: DinocoSqlCompiler,
    {
        let (sql, params) = match self.statement {
            TransactionStatement::Find(query) => compiler.compile_find_query(query),
            TransactionStatement::Insert(query) => compiler.compile_insert_query(query),
            TransactionStatement::Update(query) => compiler.compile_update_query(query),
            TransactionStatement::Delete(query) => compiler.compile_delete_query(query),
            TransactionStatement::Count(query) => compiler.compile_count_query(query),
            TransactionStatement::Noop => (String::new(), Vec::new()),
            TransactionStatement::Invalid(message) => anyhow::bail!(message),
        };

        Ok(CompiledTransactionCommand {
            sql,
            params,
            kind: self.output.kind,
            decoder: self.output.decoder,
            finish: self.output.finish,
            type_name: self.output.type_name,
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
            statement,
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
            statement,
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
