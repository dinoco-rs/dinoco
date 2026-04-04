use crate::{DinocoValue, SqlDialect};

pub struct UpdateStatement<'a, D: SqlDialect> {
    pub table: &'a str,
    pub sets: Vec<(&'a str, DinocoValue)>,
    pub wheres: Vec<(&'a str, DinocoValue)>,
    pub dialect: &'a D,
}

impl<'a, D: SqlDialect> UpdateStatement<'a, D> {
    pub fn new(dialect: &'a D) -> Self {
        Self {
            table: "",
            sets: vec![],
            wheres: vec![],
            dialect,
        }
    }

    pub fn table(mut self, table: &'a str) -> Self {
        self.table = table;

        self
    }

    pub fn set(mut self, column: &'a str, value: DinocoValue) -> Self {
        self.sets.push((column, value));

        self
    }

    pub fn sets(mut self, mut sets: Vec<(&'a str, DinocoValue)>) -> Self {
        self.sets.append(&mut sets);

        self
    }

    pub fn where_eq(mut self, column: &'a str, value: DinocoValue) -> Self {
        self.wheres.push((column, value));

        self
    }
}
