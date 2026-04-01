use crate::{DinocoValue, SqlBuilder, SqlDialect};

pub struct DropEnumStatement<'a, D: SqlDialect> {
    pub name: &'a str,
    pub dialect: &'a D,
}

impl<'a, D: SqlDialect> DropEnumStatement<'a, D> {
    pub fn new(dialect: &'a D, name: &'a str) -> Self {
        Self { name, dialect }
    }

    pub fn to_sql(&self) -> (String, Vec<DinocoValue>) {
        let mut builder = SqlBuilder::new(self.dialect, 128);

        // if self.dialect.supports_custom_enum_types() {
        //     builder.push("DROP TYPE IF EXISTS ");
        //     builder.push_identifier(self.name);
        // }

        builder.finish()
    }
}
