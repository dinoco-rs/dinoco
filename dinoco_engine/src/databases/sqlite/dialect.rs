use crate::{AdapterDialect, ColumnDefinition, ColumnType};

pub struct SqliteDialect;

impl AdapterDialect for SqliteDialect {
    fn bind_param(&self, index: usize) -> String {
        format!("?{}", index)
    }

    fn identifier(&self, v: &str) -> String {
        let escaped = v.replace('"', "\"\"");
        format!("\"{}\"", escaped)
    }

    fn literal_string(&self, v: &str) -> String {
        let escaped = v.replace('\'', "''");
        format!("'{}'", escaped)
    }

    fn column_type(
        &self,
        col: &ColumnDefinition,
        is_primary: bool,
        auto_increment: bool,
    ) -> String {
        if is_primary && auto_increment {
            return "INTEGER PRIMARY KEY AUTOINCREMENT".to_string();
        }

        let mut base_type = match &col.col_type {
            ColumnType::Integer => "INTEGER".to_string(),
            ColumnType::Float => "REAL".to_string(),
            ColumnType::Text => "TEXT".to_string(),
            ColumnType::Boolean => "INTEGER".to_string(),
            ColumnType::Json => "TEXT".to_string(),
            ColumnType::DateTime => "TEXT".to_string(),
            ColumnType::Date => "TEXT".to_string(),
            ColumnType::Bytes => "BLOB".to_string(),
            ColumnType::Enum(_) => "TEXT".to_string(),
            ColumnType::EnumInline(values) => {
                let check_values = values
                    .iter()
                    .map(|v| self.literal_string(v))
                    .collect::<Vec<_>>()
                    .join(", ");

                format!("TEXT CHECK ({} IN ({}))", col.name, check_values)
            }
        };

        if is_primary {
            base_type.push_str(" PRIMARY KEY");
        }

        base_type
    }
}
