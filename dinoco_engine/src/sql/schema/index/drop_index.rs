use crate::{DinocoValue, SqlBuilder, SqlDialect};

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

    pub fn to_sql(&self) -> (String, Vec<DinocoValue>) {
        let mut builder = SqlBuilder::new(self.dialect, 128);

        builder.push("DROP INDEX ");
        builder.push_identifier(self.index_name);

        if let Some(table) = self.table_name {
            builder.push(" ON ");
            builder.push_identifier(table);
        }

        builder.finish()
    }
}
