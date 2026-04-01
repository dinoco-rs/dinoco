use crate::SqlDialect;

pub struct DropIndexStatement<'a, D: SqlDialect> {
    pub index_name: &'a str,
    pub table_name: Option<&'a str>,
    pub dialect: &'a D,
}

impl<'a, D: SqlDialect> DropIndexStatement<'a, D> {
    pub fn new(dialect: &'a D, index_name: &'a str) -> Self {
        Self {
            index_name,
            table_name: None,
            dialect,
        }
    }

    pub fn on_table(mut self, table_name: &'a str) -> Self {
        self.table_name = Some(table_name);

        self
    }
}
