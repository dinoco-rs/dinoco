use crate::{DinocoValue, QueryDialect, SqlBuilder};

pub struct CreateIndexStatement<'a, D: QueryDialect> {
    pub table_name: &'a str,
    pub index_name: &'a str,
    pub columns: Vec<&'a str>,
    pub is_unique: bool,
    pub dialect: &'a D,
}

impl<'a, D: QueryDialect> CreateIndexStatement<'a, D> {
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

    pub fn to_sql(&self) -> (String, Vec<DinocoValue>) {
        let mut builder = SqlBuilder::new(self.dialect, 256);

        builder.push("CREATE ");

        if self.is_unique {
            builder.push("UNIQUE ");
        }

        builder.push("INDEX ");
        builder.push_identifier(self.index_name);
        builder.push(" ON ");
        builder.push_identifier(self.table_name);
        builder.push(" (");

        for (i, col) in self.columns.iter().enumerate() {
            if i > 0 {
                builder.push(", ");
            }
            builder.push_identifier(col);
        }

        builder.push(")");

        builder.finish()
    }
}
