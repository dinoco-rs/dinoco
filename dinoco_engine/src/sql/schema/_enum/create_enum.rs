use dinoco_compiler::ParsedEnum;

use crate::SqlDialect;

pub struct CreateEnumStatement<'a, D: SqlDialect> {
    pub enum_def: &'a ParsedEnum,
    pub dialect: &'a D,
}

impl<'a, D: SqlDialect> CreateEnumStatement<'a, D> {
    pub fn new(dialect: &'a D, enum_def: &'a ParsedEnum) -> Self {
        Self { enum_def, dialect }
    }
}
