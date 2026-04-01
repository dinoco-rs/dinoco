use crate::{DinocoValue, SqlBuilder, SqlDialect};
use dinoco_compiler::ParsedEnum;

pub struct CreateEnumStatement<'a, D: SqlDialect> {
    pub enum_def: &'a ParsedEnum,
    pub dialect: &'a D,
}

impl<'a, D: SqlDialect> CreateEnumStatement<'a, D> {
    pub fn new(dialect: &'a D, enum_def: &'a ParsedEnum) -> Self {
        Self { enum_def, dialect }
    }

    pub fn to_sql(&self) -> (String, Vec<DinocoValue>) {
        let mut builder = SqlBuilder::new(self.dialect, 256);

        // Apenas bancos que suportam tipos customizados (Postgres) geram esse SQL
        // if self.dialect.supports_custom_enum_types() {
        //     builder.push("CREATE TYPE ");
        //     builder.push_identifier(&self.enum_def.name);
        //     builder.push(" AS ENUM (");

        //     for (i, value) in self.enum_def.values.iter().enumerate() {
        //         if i > 0 {
        //             builder.push(", ");
        //         }
        //         builder.push(&format!("'{}'", value));
        //     }

        //     builder.push(")");
        // }

        builder.finish()
    }
}
