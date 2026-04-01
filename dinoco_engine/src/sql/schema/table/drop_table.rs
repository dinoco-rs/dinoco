use crate::SqlDialect;

pub struct DropTableStatement<'a, D: SqlDialect> {
    pub table_name: &'a str,
    pub cascade: bool,
    pub dialect: &'a D,
}

impl<'a, D: SqlDialect> DropTableStatement<'a, D> {
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
}
