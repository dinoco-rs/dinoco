use crate::DinocoValue;

#[derive(Debug, Clone, Default)]
pub struct InsertStatement {
    pub table: String,
    pub columns: Vec<String>,
    pub rows: Vec<Vec<DinocoValue>>,
}

impl InsertStatement {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn into(mut self, table: impl Into<String>) -> Self {
        self.table = table.into();
        self
    }

    pub fn columns(mut self, columns: &[&str]) -> Self {
        self.columns = columns.iter().map(|column| column.to_string()).collect();
        self
    }

    pub fn value(mut self, row: Vec<DinocoValue>) -> Self {
        self.rows.push(row);
        self
    }

    pub fn values(mut self, rows: Vec<Vec<DinocoValue>>) -> Self {
        self.rows.extend(rows);
        self
    }
}
