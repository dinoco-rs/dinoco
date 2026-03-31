use super::SqlBuilder;
use crate::{DinocoValue, QueryDialect};

pub struct DropTableStatement<'a, D: QueryDialect> {
    pub table_name: &'a str,
    pub cascade: bool,
    pub dialect: &'a D,
}

impl<'a, D: QueryDialect> DropTableStatement<'a, D> {
    pub fn new(dialect: &'a D, table_name: &'a str) -> Self {
        Self {
            table_name,
            cascade: false,
            dialect,
        }
    }

    pub fn cascade(mut self) -> Self {
        self.cascade = true;

        self
    }

    pub fn to_sql(&self) -> (String, Vec<DinocoValue>) {
        let mut builder = SqlBuilder::new(self.dialect, 128);

        builder.push("DROP TABLE ");

        builder.push_identifier(self.table_name);

        if self.cascade {
            builder.push(" CASCADE");
        }

        builder.finish()
    }
}
