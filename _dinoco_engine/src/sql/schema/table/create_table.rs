use crate::{ColumnDefinition, ConstraintDefinition, SqlDialect};

pub struct CreateTableStatement<'a, D: SqlDialect> {
    pub table_name: &'a str,
    pub columns: Vec<ColumnDefinition<'a>>,
    pub constraints: Vec<ConstraintDefinition<'a>>,
    pub dialect: &'a D,
}

impl<'a, D: SqlDialect> CreateTableStatement<'a, D> {
    pub fn new(dialect: &'a D, table_name: &'a str) -> Self {
        Self {
            table_name,
            columns: vec![],
            constraints: vec![],
            dialect,
        }
    }

    pub fn column(mut self, column: ColumnDefinition<'a>) -> Self {
        self.columns.push(column);

        self
    }

    pub fn add_constraint(mut self, constraint: ConstraintDefinition<'a>) -> Self {
        self.constraints.push(constraint);

        self
    }
}
