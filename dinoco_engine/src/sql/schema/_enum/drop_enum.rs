use crate::SqlDialect;

pub struct DropEnumStatement<'a, D: SqlDialect> {
    pub name: &'a str,
    pub dialect: &'a D,
}

impl<'a, D: SqlDialect> DropEnumStatement<'a, D> {
    pub fn new(dialect: &'a D, name: &'a str) -> Self {
        Self { name, dialect }
    }
}
