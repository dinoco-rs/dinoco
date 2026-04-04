use crate::SqlDialect;

pub struct CreateIndexStatement<'a, D: SqlDialect> {
    pub table_name: &'a str,
    pub index_name: &'a str,
    pub columns: Vec<&'a str>,
    pub is_unique: bool,
    pub dialect: &'a D,
}

impl<'a, D: SqlDialect> CreateIndexStatement<'a, D> {
    pub fn new(dialect: &'a D, table_name: &'a str, index_name: &'a str) -> Self {
        Self {
            table_name,
            index_name,
            columns: vec![],
            is_unique: false,
            dialect,
        }
    }

    pub fn column(mut self, column_name: &'a str) -> Self {
        self.columns.push(column_name);

        self
    }

    pub fn columns(mut self, columns: Vec<&'a str>) -> Self {
        self.columns.extend(columns);

        self
    }

    pub fn unique(mut self) -> Self {
        self.is_unique = true;

        self
    }
}
