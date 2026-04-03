use crate::SqlDialect;

pub struct DropEnumStatement<'a, D: SqlDialect> {
    pub name: &'a str,
    pub dialect: &'a D,
    pub cascade: bool,
}

impl<'a, D: SqlDialect> DropEnumStatement<'a, D> {
    pub fn new(dialect: &'a D, name: &'a str) -> Self {
        Self { name, dialect, cascade: false }
    }

    pub fn cascade(mut self) -> Self {
        self.cascade = true;

        self
    }
}
