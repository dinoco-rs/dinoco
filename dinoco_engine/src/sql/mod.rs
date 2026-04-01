use crate::{DinocoValue, QueryDialect};

mod alter_table;
mod create_enum;
mod create_index;
mod create_table;
mod data;
mod drop_enum;
mod drop_index;
mod drop_table;
mod helpers;
mod select;

pub use alter_table::*;
pub use create_enum::*;
pub use create_index::*;
pub use create_table::*;
pub use data::*;
pub use drop_enum::*;
pub use drop_index::*;
pub use drop_table::*;
pub use helpers::*;
pub use select::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderDirection {
    Asc,
    Desc,
}

pub enum BinaryOperator {
    Eq,
    Neq,
    Gt,
    Lt,
    Gte,
    Lte,
    Like,
}

pub enum Expression {
    Column(String),
    String(String),
    Value(DinocoValue),
    IsNull(Box<Expression>),
    IsNotNull(Box<Expression>),
    BinaryOp {
        left: Box<Expression>,
        op: BinaryOperator,
        right: Box<Expression>,
    },
}

pub struct SqlBuilder<'a, D: QueryDialect> {
    buffer: String,
    parameters: Vec<DinocoValue>,
    dialect: &'a D,
}

impl<'a, D: QueryDialect> SqlBuilder<'a, D> {
    pub fn new(dialect: &'a D, size: usize) -> Self {
        Self {
            buffer: String::with_capacity(size),
            parameters: vec![],
            dialect,
        }
    }

    pub fn push(&mut self, text: &str) {
        self.buffer.push_str(text);
    }

    pub fn push_identifier(&mut self, identifier: &str) {
        self.buffer.push_str(&self.dialect.identifier(identifier));
    }

    pub fn push_string(&mut self, string: &str) {
        self.buffer.push_str(&self.dialect.string(string));
    }

    pub fn push_bind_param(&mut self, value: DinocoValue) {
        self.parameters.push(value);

        self.buffer.push_str(&self.dialect.bind_param(self.parameters.len()));
    }

    pub fn push_operator(&mut self, op: &BinaryOperator) {
        let op_str = match op {
            BinaryOperator::Eq => "=",
            BinaryOperator::Neq => "<>",
            BinaryOperator::Gt => ">",
            BinaryOperator::Lt => "<",
            BinaryOperator::Gte => ">=",
            BinaryOperator::Lte => "<=",
            BinaryOperator::Like => "LIKE",
        };

        self.buffer.push_str(&format!(" {} ", op_str));
    }

    pub fn finish(self) -> (String, Vec<DinocoValue>) {
        (self.buffer, self.parameters)
    }
}
