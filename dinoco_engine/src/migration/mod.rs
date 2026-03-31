pub mod differ;
pub mod mapper;
pub mod sql;
pub mod step;

pub use step::MigrationStep;

use crate::DinocoAdapter;
use dinoco_compiler::ParsedSchema;

pub struct Migration<T: DinocoAdapter> {
    pub adapter: T,
    pub old_schema: Option<ParsedSchema>,
    pub new_schema: ParsedSchema,
}

impl<T> Migration<T>
where
    T: DinocoAdapter,
{
    pub fn new(adapter: T, old_schema: Option<ParsedSchema>, new_schema: ParsedSchema) -> Self {
        Self { adapter, old_schema, new_schema }
    }

    pub fn to_up_sql(&self, changes: Vec<MigrationStep>) -> String {
        sql::generate_up_sql(&self.adapter, changes)
    }

    pub fn diff(&self) -> Vec<MigrationStep> {
        differ::calculate_diff(&self.old_schema, &self.new_schema)
    }
}
