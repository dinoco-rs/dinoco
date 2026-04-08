use crate::{DinocoValue, Expression, UpdateBatchItem};

#[derive(Debug, Clone, PartialEq)]
pub enum UpdateOperation {
    Set(DinocoValue),
    Increment(DinocoValue),
    Decrement(DinocoValue),
    Multiply(DinocoValue),
    Division(DinocoValue),
}

#[derive(Debug, Clone, PartialEq)]
pub struct UpdateAssignment {
    pub column: String,
    pub operation: UpdateOperation,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateTarget {
    pub primary_keys: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct UpdateStatement {
    pub table: String,
    pub sets: Vec<UpdateAssignment>,
    pub conditions: Vec<Expression>,
    pub batches: Vec<UpdateBatchItem>,
    pub target: Option<UpdateTarget>,
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
        self.sets.push(UpdateAssignment { column: column.into(), operation: UpdateOperation::Set(value.into()) });
        self
    }

    pub fn increment(mut self, column: impl Into<String>, value: impl Into<DinocoValue>) -> Self {
        self.sets.push(UpdateAssignment { column: column.into(), operation: UpdateOperation::Increment(value.into()) });
        self
    }

    pub fn decrement(mut self, column: impl Into<String>, value: impl Into<DinocoValue>) -> Self {
        self.sets.push(UpdateAssignment { column: column.into(), operation: UpdateOperation::Decrement(value.into()) });
        self
    }

    pub fn multiply(mut self, column: impl Into<String>, value: impl Into<DinocoValue>) -> Self {
        self.sets.push(UpdateAssignment { column: column.into(), operation: UpdateOperation::Multiply(value.into()) });
        self
    }

    pub fn division(mut self, column: impl Into<String>, value: impl Into<DinocoValue>) -> Self {
        self.sets.push(UpdateAssignment { column: column.into(), operation: UpdateOperation::Division(value.into()) });
        self
    }

    pub fn target_first_match(mut self, primary_keys: &[&str]) -> Self {
        self.target = Some(UpdateTarget { primary_keys: primary_keys.iter().map(|item| item.to_string()).collect() });
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
