use crate::{DinocoValue, SqlBuilder, SqlDialect};

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

    pub fn to_sql(&self) -> (String, Vec<DinocoValue>) {
        let estimated_size = self.table.len() + (self.columns.len() * 20) + (self.rows.len() * self.columns.len() * 20) + 100;

        let mut builder = SqlBuilder::new(self.dialect, estimated_size);

        builder.push("INSERT INTO ");
        builder.push(self.table);

        if let Some((first, rest)) = self.columns.split_first() {
            builder.push(" (");
            builder.push(first);
            for col in rest {
                builder.push(", ");
                builder.push(col);
            }
            builder.push(")");
        }

        builder.push(" VALUES ");

        for (i, row) in self.rows.iter().enumerate() {
            if i > 0 {
                builder.push(", ");
            }

            builder.push("(");
            if let Some((first_val, rest_vals)) = row.split_first() {
                builder.push_bind_param(first_val.clone());
                for val in rest_vals {
                    builder.push(", ");
                    builder.push_bind_param(val.clone());
                }
            }
            builder.push(")");
        }

        builder.finish()
    }
}
