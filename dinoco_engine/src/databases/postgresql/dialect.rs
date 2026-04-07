use crate::{AdapterDialect, ColumnDefinition, ColumnType, DinocoValue};

pub struct PostgresDialect;

impl AdapterDialect for PostgresDialect {
    fn bind_param(&self, index: usize) -> String {
        format!("${}", index)
    }

    fn cast_numeric_for_division(&self, expr: &str) -> String {
        format!("CAST({expr} AS DOUBLE PRECISION)")
    }

    fn bind_value(&self, index: usize, value: &DinocoValue) -> String {
        match value {
            DinocoValue::Enum(type_name, _) => {
                format!("{}::{}", self.bind_param(index), self.identifier(type_name))
            }
            _ => self.bind_param(index),
        }
    }

    fn identifier(&self, v: &str) -> String {
        let escaped = v.replace('"', "\"\"");

        format!("\"{}\"", escaped)
    }

    fn literal_string(&self, v: &str) -> String {
        let escaped = v.replace('\'', "''");
        format!("'{}'", escaped)
    }

    fn supports_native_enums(&self) -> bool {
        true
    }

    fn column_type(&self, col: &ColumnDefinition, is_primary: bool, auto_increment: bool) -> String {
        let mut base_type = match &col.col_type {
            ColumnType::Integer => "BIGINT".to_string(),
            ColumnType::Float => "DOUBLE PRECISION".to_string(),
            ColumnType::Text => "TEXT".to_string(),
            ColumnType::Boolean => "BOOLEAN".to_string(),
            ColumnType::Json => "JSONB".to_string(),
            ColumnType::DateTime => "TIMESTAMP".to_string(),
            ColumnType::Date => "DATE".to_string(),
            ColumnType::Bytes => "BYTEA".to_string(),

            ColumnType::Enum(name) => self.identifier(name),
            ColumnType::EnumInline(_) => "TEXT".into(),
        };

        if auto_increment {
            base_type.push_str(" GENERATED ALWAYS AS IDENTITY");
        }

        if is_primary {
            base_type.push_str(" PRIMARY KEY");
        }

        base_type
    }
}
