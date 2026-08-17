use std::sync::Arc;

use anyhow::{Context, anyhow};
use mysql_async::prelude::Queryable;
use mysql_async::{Pool, TxOpts, Value};

mod compiler;

use crate::{
    CompiledTransactionCommand, CompiledTransactionStatement, DinocoAdapter, DinocoRowModel, DinocoValue,
    RawTransactionOutput, TransactionCommandKind, TransactionResults,
};

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
        let mut conn = self.pool.get_conn().await.context("Failed to get mysql connection from pool")?;
        let rows: Vec<mysql_async::Row> = conn.exec(query, mysql_params(params)).await?;

        rows.into_iter()
            .map(|row| M::from_mysql_row(&row).ok_or_else(|| anyhow!("Failed to parse mysql row")))
            .collect()
    }

    async fn query_optional<M>(&self, query: &str, params: &[DinocoValue]) -> anyhow::Result<Vec<M>>
    where
        M: DinocoRowModel,
    {
        let mut conn = self.pool.get_conn().await.context("Failed to get mysql connection from pool")?;
        let rows: Vec<mysql_async::Row> = conn.exec(query, mysql_params(params)).await?;

        Ok(rows.into_iter().filter_map(|row| M::from_mysql_row(&row)).collect())
    }

    async fn execute(&self, query: &str, params: &[DinocoValue]) -> anyhow::Result<usize> {
        let mut conn = self.pool.get_conn().await.context("Failed to get mysql connection from pool")?;
        conn.exec_drop(query, mysql_params(params)).await?;

        Ok(conn.affected_rows() as usize)
    }
}

impl MySqlAdapter {
    pub fn new(url: impl Into<String>) -> Self {
        let url = url.into();
        let pool = Pool::new(url.as_str());

        Self { url, pool: Arc::new(pool), with_logger: false }
    }

    pub(crate) fn set_logger(&mut self, enabled: bool) {
        self.with_logger = enabled;
    }

    pub(crate) fn logger_enabled(&self) -> bool {
        self.with_logger
    }

    pub(crate) async fn execute_compiled_transaction(
        &self,
        commands: Vec<CompiledTransactionCommand>,
    ) -> anyhow::Result<TransactionResults> {
        let mut conn = self.pool.get_conn().await.context("Failed to get mysql connection from pool")?;
        let mut transaction = conn.start_transaction(TxOpts::default()).await?;
        let mut values = Vec::with_capacity(commands.len());

        for command in commands {
            let execution =
                execute_transaction_command(&mut transaction, &command).await.and_then(|raw| command.finish(raw));

            match execution {
                Ok(value) => values.push(value),
                Err(error) => {
                    transaction.rollback().await.context("Failed to roll back mysql transaction")?;
                    return Err(error);
                }
            }
        }

        transaction.commit().await?;
        Ok(TransactionResults::new(values))
    }

    pub async fn query_count(&self, query: &str, params: &[DinocoValue]) -> anyhow::Result<i64> {
        let mut conn = self.pool.get_conn().await.context("Failed to get mysql connection from pool")?;
        let count = conn.exec_first::<i64, _, _>(query, mysql_params(params)).await?.unwrap_or_default();

        Ok(count)
    }
}

async fn execute_transaction_command(
    transaction: &mut mysql_async::Transaction<'_>,
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
    transaction: &mut mysql_async::Transaction<'_>,
    command: &CompiledTransactionStatement,
) -> anyhow::Result<RawTransactionOutput> {
    if command.sql.is_empty() {
        return match command.kind {
            TransactionCommandKind::Rows => Ok(RawTransactionOutput::Rows(Vec::new())),
            TransactionCommandKind::Execute => Ok(RawTransactionOutput::Affected(0)),
            TransactionCommandKind::Count => Ok(RawTransactionOutput::Count(0)),
        };
    }

    match command.kind {
        TransactionCommandKind::Rows => {
            let decoder =
                command.decoder.ok_or_else(|| anyhow!("Dinoco transaction query is missing its mysql row decoder."))?;
            let rows = transaction.exec::<mysql_async::Row, _, _>(&command.sql, mysql_params(&command.params)).await?;
            let values = rows
                .iter()
                .map(|row| (decoder.mysql)(row).ok_or_else(|| anyhow!("Failed to parse mysql transaction row")))
                .collect::<anyhow::Result<Vec<_>>>()?;
            Ok(RawTransactionOutput::Rows(values))
        }
        TransactionCommandKind::Execute => {
            transaction.exec_drop(&command.sql, mysql_params(&command.params)).await?;
            Ok(RawTransactionOutput::Affected(transaction.affected_rows() as usize))
        }
        TransactionCommandKind::Count => {
            let total =
                transaction.exec_first::<i64, _, _>(&command.sql, mysql_params(&command.params)).await?.unwrap_or(0);
            Ok(RawTransactionOutput::Count(total))
        }
    }
}

fn mysql_params(params: &[DinocoValue]) -> Vec<Value> {
    params
        .iter()
        .map(|param| match param {
            DinocoValue::Null => Value::NULL,
            DinocoValue::Integer(value) => Value::Int(*value),
            DinocoValue::Float(value) => Value::Double(*value),
            DinocoValue::String(value) => Value::Bytes(value.clone().into_bytes()),
            DinocoValue::Enum(_, value) => Value::Bytes(value.clone().into_bytes()),
            DinocoValue::Boolean(value) => Value::Int(if *value { 1 } else { 0 }),
            DinocoValue::Bytes(value) => Value::Bytes(value.clone()),
            DinocoValue::Json(value) => Value::Bytes(value.to_string().into_bytes()),
            DinocoValue::DateTime(value) => Value::Bytes(value.naive_utc().to_string().into_bytes()),
            DinocoValue::Date(value) => Value::Bytes(value.to_string().into_bytes()),
        })
        .collect()
}

impl mysql_common::prelude::FromValue for DinocoValue {
    type Intermediate = DinocoValue;
}

impl TryFrom<Value> for DinocoValue {
    type Error = mysql_common::value::convert::FromValueError;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        match value {
            Value::NULL => Ok(DinocoValue::Null),
            Value::Bytes(value) => {
                String::from_utf8(value.clone()).map(DinocoValue::String).or(Ok(DinocoValue::Bytes(value)))
            }
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
