use crate::SqlDialect;

pub struct CreateEnumStatement<'a, D: SqlDialect> {
    pub name: &'a str,
    pub variants: Vec<String>,
    pub dialect: &'a D,
}

impl<'a, D: SqlDialect> CreateEnumStatement<'a, D> {
    pub fn new(dialect: &'a D, name: &'a str, variants: Vec<String>) -> Self {
        Self { name, variants, dialect }
    }
}
