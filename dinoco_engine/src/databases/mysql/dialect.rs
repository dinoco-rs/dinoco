use crate::{AdapterDialect, ColumnDefinition, ColumnType};

pub struct MySqlDialect;

impl AdapterDialect for MySqlDialect {
    fn bind_param(&self, _index: usize) -> String {
        "?".to_string()
    }

    fn identifier(&self, v: &str) -> String {
        let escaped = v.replace('`', "``");
        format!("`{}`", escaped)
    }

    fn literal_string(&self, v: &str) -> String {
        let escaped = v.replace('\'', "''");
        format!("'{}'", escaped)
    }

    fn offset_without_limit(&self) -> String {
        "18446744073709551615".to_string()
    }

    fn column_type(
        &self,
        col: &ColumnDefinition,
        is_primary: bool,
        auto_increment: bool,
    ) -> String {
        let base_type = match &col.col_type {
            ColumnType::Integer => "BIGINT".to_string(),
            ColumnType::Float => "DOUBLE PRECISION".to_string(),
            ColumnType::Text => "VARCHAR(255)".to_string(),
            ColumnType::Boolean => "TINYINT(1)".to_string(),
            ColumnType::Json => "JSON".to_string(),
            ColumnType::DateTime => "TIMESTAMP".to_string(),
            ColumnType::Bytes => "BLOB".to_string(),
            ColumnType::Enum(name) => {
                format!("VARCHAR(255) /* enum {} */", name)
            }
            ColumnType::EnumInline(values) => {
                let safe_values = values
                    .iter()
                    .map(|v| format!("'{}'", v.replace('\'', "''")))
                    .collect::<Vec<_>>()
                    .join(", ");

                format!("ENUM({})", safe_values)
            }
        };

        let mut definition = base_type;

        if auto_increment {
            definition.push_str(" AUTO_INCREMENT");
        }

        if is_primary {
            definition.push_str(" PRIMARY KEY");
        }

        definition
    }
}
