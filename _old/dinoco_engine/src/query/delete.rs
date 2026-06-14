use crate::Expression;

#[derive(Debug, Clone, Default)]
pub struct DeleteStatement {
    pub table: String,
    pub conditions: Vec<Expression>,
    pub batches: Vec<Vec<Expression>>,
}

impl DeleteStatement {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from(mut self, table: impl Into<String>) -> Self {
        self.table = table.into();

        self
    }

    pub fn condition(mut self, condition: Expression) -> Self {
        self.conditions.push(condition);

        self
    }

    pub fn delete_where(mut self, conditions: Vec<Expression>) -> Self {
        self.batches.push(conditions);

        self
    }
}
