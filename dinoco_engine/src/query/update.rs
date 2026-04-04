use crate::{DinocoValue, Expression, UpdateBatchItem};

#[derive(Debug, Clone, Default)]
pub struct UpdateStatement {
    pub table: String,
    pub sets: Vec<(String, DinocoValue)>,
    pub conditions: Vec<Expression>,
    pub batches: Vec<UpdateBatchItem>,
}

impl UpdateStatement {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn table(mut self, table: impl Into<String>) -> Self {
        self.table = table.into();
        self
    }

    pub fn set(mut self, column: impl Into<String>, value: impl Into<DinocoValue>) -> Self {
        self.sets.push((column.into(), value.into()));
        self
    }

    pub fn condition(mut self, condition: Expression) -> Self {
        self.conditions.push(condition);
        self
    }

    pub fn batch(mut self, item: UpdateBatchItem) -> Self {
        self.batches.push(item);

        self
    }
}
