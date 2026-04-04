use crate::{DinocoValue, SqlDialect};

pub struct InsertStatement<'a, D: SqlDialect> {
    pub table: &'a str,
    pub columns: &'a [&'a str],
    pub rows: Vec<Vec<DinocoValue>>,
    pub dialect: &'a D,
}

impl<'a, D: SqlDialect> InsertStatement<'a, D> {
    pub fn new(dialect: &'a D) -> Self {
        Self {
            table: "",
            columns: &[],
            rows: vec![],
            dialect,
        }
    }

    pub fn into(mut self, table: &'a str) -> Self {
        self.table = table;

        self
    }

    pub fn columns(mut self, columns: &'a [&'a str]) -> Self {
        self.columns = columns;

        self
    }

    pub fn value(mut self, row: Vec<DinocoValue>) -> Self {
        self.rows.push(row);

        self
    }

    pub fn values(mut self, mut rows: Vec<Vec<DinocoValue>>) -> Self {
        self.rows.append(&mut rows);

        self
    }
}
