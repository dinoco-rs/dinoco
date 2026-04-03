use crate::SqlDialect;

pub struct AlterEnumStatement<'a, D: SqlDialect> {
    pub name: &'a str,
    pub old_variants: &'a [String],
    pub new_variants: &'a [String],
    pub dialect: &'a D,
}

impl<'a, D: SqlDialect> AlterEnumStatement<'a, D> {
    pub fn new(dialect: &'a D, name: &'a str, old_variants: &'a [String], new_variants: &'a [String]) -> Self {
        Self {
            name,
            old_variants,
            new_variants,
            dialect,
        }
    }
}
