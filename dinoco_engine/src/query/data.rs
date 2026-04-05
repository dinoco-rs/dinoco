use crate::DinocoValue;

pub type Query = (String, Vec<DinocoValue>);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderDirection {
    Asc,
    Desc,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOperator {
    Eq,
    Neq,
    Gt,
    Lt,
    Gte,
    Lte,
    Like,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expression {
    Column(String),
    Value(DinocoValue),
    Raw(String),
    IsNull(Box<Expression>),
    IsNotNull(Box<Expression>),
    In { expr: Box<Expression>, values: Vec<DinocoValue> },
    And(Vec<Expression>),
    Or(Vec<Expression>),
    BinaryOp { left: Box<Expression>, op: BinaryOperator, right: Box<Expression> },
}

impl Expression {
    pub fn raw(value: impl Into<String>) -> Self {
        Self::Raw(value.into())
    }

    pub fn value(value: impl Into<DinocoValue>) -> Self {
        Self::Value(value.into())
    }

    pub fn eq(self, value: impl Into<DinocoValue>) -> Self {
        Self::BinaryOp { left: Box::new(self), op: BinaryOperator::Eq, right: Box::new(Self::Value(value.into())) }
    }

    pub fn neq(self, value: impl Into<DinocoValue>) -> Self {
        Self::BinaryOp { left: Box::new(self), op: BinaryOperator::Neq, right: Box::new(Self::Value(value.into())) }
    }

    pub fn gt(self, value: impl Into<DinocoValue>) -> Self {
        Self::BinaryOp { left: Box::new(self), op: BinaryOperator::Gt, right: Box::new(Self::Value(value.into())) }
    }

    pub fn lt(self, value: impl Into<DinocoValue>) -> Self {
        Self::BinaryOp { left: Box::new(self), op: BinaryOperator::Lt, right: Box::new(Self::Value(value.into())) }
    }

    pub fn gte(self, value: impl Into<DinocoValue>) -> Self {
        Self::BinaryOp { left: Box::new(self), op: BinaryOperator::Gte, right: Box::new(Self::Value(value.into())) }
    }

    pub fn lte(self, value: impl Into<DinocoValue>) -> Self {
        Self::BinaryOp { left: Box::new(self), op: BinaryOperator::Lte, right: Box::new(Self::Value(value.into())) }
    }

    pub fn like(self, value: impl Into<DinocoValue>) -> Self {
        Self::BinaryOp { left: Box::new(self), op: BinaryOperator::Like, right: Box::new(Self::Value(value.into())) }
    }

    pub fn is_null(self) -> Self {
        Self::IsNull(Box::new(self))
    }

    pub fn is_not_null(self) -> Self {
        Self::IsNotNull(Box::new(self))
    }

    pub fn and(expressions: Vec<Expression>) -> Self {
        Self::And(expressions)
    }

    pub fn or(expressions: Vec<Expression>) -> Self {
        Self::Or(expressions)
    }

    pub fn in_values(self, values: Vec<DinocoValue>) -> Self {
        Self::In { expr: Box::new(self), values }
    }
}

#[derive(Debug, Clone)]
pub struct UpdateBatchItem {
    pub conditions: Vec<Expression>,
    pub values: Vec<(String, DinocoValue)>,
}
