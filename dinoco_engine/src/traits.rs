use async_trait::async_trait;

use crate::ColumnType;
use crate::{DinocoResult, DinocoStream, DinocoValue};

#[async_trait]
pub trait DinocoAdapter: Sized {
    type Dialect: QueryDialect;

    fn dialect(&self) -> &Self::Dialect;

    async fn connect(url: String) -> DinocoResult<Self>;
    async fn execute(&self, query: &str, params: &[DinocoValue]) -> DinocoResult<()>;
    async fn query_as<T: DinocoRow>(&self, query: &str, params: &[DinocoValue]) -> DinocoResult<Vec<T>>;

    async fn stream_as<T: DinocoRow + Send + 'static>(&self, query: &str, params: &[DinocoValue]) -> DinocoStream<T>;
}

pub trait FromDinocoValue: Sized {
    fn from_value(value: &DinocoValue) -> DinocoResult<Self>;
}

pub trait RowExt {
    fn get_value<T: FromDinocoValue>(&self, index: usize) -> DinocoResult<T>;
}

pub trait DinocoDatabaseRow {
    fn get_i64(&self, idx: usize) -> DinocoResult<i64>;
    fn get_string(&self, idx: usize) -> DinocoResult<String>;
    fn get_bool(&self, idx: usize) -> DinocoResult<bool>;
    fn get_f64(&self, idx: usize) -> DinocoResult<f64>;
    fn get_bytes(&self, idx: usize) -> DinocoResult<Vec<u8>>;

    fn get<T: DinocoType>(&self, idx: usize) -> DinocoResult<T>;
}

pub trait DinocoType: Sized {
    fn from_row<R: DinocoDatabaseRow>(row: &R, idx: usize) -> DinocoResult<Self>;
}

pub trait DinocoRow: Sized {
    fn from_row<R: DinocoDatabaseRow>(row: &R) -> DinocoResult<Self>;
}

pub trait QueryDialect {
    fn bind_param(&self, index: usize) -> String;
    fn identifier(&self, v: &str) -> String;
    fn string(&self, v: &str) -> String;

    fn column_type(&self, t: &ColumnType, is_primary: bool, auto_increment: bool) -> String;
    fn modify_column(&self) -> String;

    fn get_public_table(&self) -> String;
    fn cast_boolean(&self, column: String) -> String;
}
