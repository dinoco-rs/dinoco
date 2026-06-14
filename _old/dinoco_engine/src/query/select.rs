use crate::{Expression, OrderDirection};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SelectStatement {
    pub select: Vec<String>,
    pub from: String,
    pub conditions: Vec<Expression>,
    pub limit: Option<usize>,
    pub skip: Option<usize>,
    pub order_by: Vec<(String, OrderDirection)>,
}

impl SelectStatement {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn select(mut self, columns: &[&str]) -> Self {
        self.select = columns.iter().map(|column| column.to_string()).collect();

        self
    }

    pub fn from(mut self, table: impl Into<String>) -> Self {
        self.from = table.into();

        self
    }

    pub fn condition(mut self, condition: Expression) -> Self {
        self.conditions.push(condition);

        self
    }

    pub fn conditions(mut self, conditions: Vec<Expression>) -> Self {
        self.conditions.extend(conditions);

        self
    }

    pub fn limit(mut self, limit: usize) -> Self {
        self.limit = Some(limit);

        self
    }

    pub fn skip(mut self, skip: usize) -> Self {
        self.skip = Some(skip);

        self
    }

    pub fn order_by(mut self, column: impl Into<String>, direction: OrderDirection) -> Self {
        self.order_by.push((column.into(), direction));

        self
    }
}
