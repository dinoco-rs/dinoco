use async_trait::async_trait;
use dinoco_compiler::ParsedSchema;

use crate::{
    ColumnDefinition, DatabaseColumn, DatabaseEnumRaw, DatabaseForeignKey, DatabaseIndex, DatabaseParsedTable,
    DinocoClientConfig, DinocoError, DinocoResult, DinocoValue, MigrationStep,
};

#[async_trait]
pub trait DinocoAdapter: Sized {
    type Dialect: AdapterDialect;

    fn dialect(&self) -> &Self::Dialect;

    async fn connect(url: String, config: DinocoClientConfig) -> DinocoResult<Self>;

    async fn execute(&self, query: &str, params: &[DinocoValue]) -> DinocoResult<()>;
    async fn execute_script(&self, sql_content: &str) -> DinocoResult<()> {
        let clean_sql = sql_content.trim();

        if clean_sql.is_empty() {
            return Ok(());
        }

        self.execute(clean_sql, &[]).await
    }

    async fn query_as<T: DinocoRow>(&self, query: &str, params: &[DinocoValue]) -> DinocoResult<Vec<T>>;
}

#[async_trait]
pub trait DinocoAdapterHandler: Sized {
    async fn fetch_tables(&self) -> DinocoResult<Vec<DatabaseParsedTable>>;
    async fn fetch_columns(&self, table_name: String) -> DinocoResult<Vec<DatabaseColumn>>;
    async fn fetch_foreign_keys(&self) -> DinocoResult<Vec<DatabaseForeignKey>>;
    async fn fetch_enums(&self) -> DinocoResult<Vec<DatabaseEnumRaw>>;
    async fn fetch_indexes(&self) -> DinocoResult<Vec<DatabaseIndex>>;

    async fn reset_database(&self) -> DinocoResult<()>;
}

pub trait AdapterDialect {
    fn bind_param(&self, index: usize) -> String;
    fn bind_value(&self, index: usize, value: &DinocoValue) -> String {
        let _ = value;

        self.bind_param(index)
    }
    fn identifier(&self, v: &str) -> String;
    fn literal_string(&self, v: &str) -> String;
    fn column_type(&self, t: &ColumnDefinition, is_primary: bool, auto_increment: bool) -> String;
    fn offset_without_limit(&self) -> String {
        "-1".to_string()
    }

    fn supports_native_enums(&self) -> bool {
        false
    }
}

pub trait DinocoGenericRow {
    fn get_value(&self, idx: usize) -> DinocoResult<DinocoValue>;

    fn get_optional<T>(&self, idx: usize) -> DinocoResult<Option<T>>
    where
        T: TryFrom<DinocoValue, Error = DinocoError>,
    {
        let value = self.get_value(idx)?;

        match value {
            DinocoValue::Null => Ok(None),
            v => Ok(Some(T::try_from(v)?)),
        }
    }

    fn get<T>(&self, idx: usize) -> DinocoResult<T>
    where
        T: TryFrom<DinocoValue, Error = DinocoError>,
    {
        let value = self.get_value(idx)?;

        T::try_from(value)
    }
}

pub trait DinocoRow: Sized + Send + 'static {
    fn from_row<R: DinocoGenericRow>(row: &R) -> DinocoResult<Self>;
}

pub trait MigrationExecutor {
    fn build_step(&self, step: &MigrationStep, schema: &ParsedSchema) -> Vec<String>;
    fn build_reverse_step(&self, step: &MigrationStep, schema: &ParsedSchema) -> Vec<String>;

    fn build_migration(&self, steps: &[MigrationStep], schema: &ParsedSchema, reverse: bool) -> Vec<String> {
        let mut sqls = Vec::new();

        for step in steps {
            let mut step_sqls =
                if reverse { self.build_reverse_step(step, schema) } else { self.build_step(step, schema) };

            for sql in &mut step_sqls {
                let trimmed = sql.trim_end();

                if !trimmed.ends_with(';') {
                    *sql = format!("{};", trimmed);
                }
            }

            sqls.extend(step_sqls);
        }

        sqls
    }
}
