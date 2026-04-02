pub mod differ;
pub mod mapper;
pub mod migration_sql;
pub mod step;

pub use step::MigrationStep;

use crate::{DinocoAdapter, SqlDialectBuilders};
use dinoco_compiler::ParsedSchema;

pub struct Migration<'a, T>
where
    T: DinocoAdapter,
    T::Dialect: SqlDialectBuilders,
{
    pub adapter: &'a T,
    pub old_schema: Option<ParsedSchema>,
    pub new_schema: ParsedSchema,
}

impl<'a, T> Migration<'a, T>
where
    T: DinocoAdapter,
    T::Dialect: SqlDialectBuilders,
{
    pub fn new(adapter: &'a T, old_schema: Option<ParsedSchema>, new_schema: ParsedSchema) -> Self {
        Self { adapter, old_schema, new_schema }
    }

    pub fn to_up_sql(&self, changes: Vec<MigrationStep>) -> Vec<String> {
        migration_sql::generate_up_sql(self.adapter, changes)
    }

    pub fn to_down_sql(&self, changes: Vec<MigrationStep>) -> Vec<String> {
        migration_sql::generate_down_sql(self.adapter, changes)
    }

    pub fn diff(&self) -> Vec<MigrationStep> {
        differ::calculate_diff(&self.old_schema, &self.new_schema)
    }
}
