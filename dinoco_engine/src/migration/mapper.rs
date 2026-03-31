use crate::{ColumnDefault, ColumnDefinition, ColumnType, DinocoValue};
use dinoco_compiler::{FunctionCall, ParsedField, ParsedFieldDefault, ParsedFieldType, ReferentialAction};

pub fn map_field_to_definition<'a>(field: &'a ParsedField) -> ColumnDefinition<'a> {
    ColumnDefinition {
        name: field.name.as_str(),
        col_type: map_column_type(&field.field_type),
        primary_key: field.is_primary_key,
        not_null: !field.is_optional,
        auto_increment: matches!(field.default_value, ParsedFieldDefault::Function(FunctionCall::AutoIncrement)),
        default: map_default(&field.default_value),
    }
}

pub fn map_column_type(field_type: &ParsedFieldType) -> ColumnType {
    match field_type {
        ParsedFieldType::String => ColumnType::Text,
        ParsedFieldType::Boolean => ColumnType::Boolean,
        ParsedFieldType::Integer => ColumnType::Integer,
        ParsedFieldType::Float => ColumnType::Float,
        ParsedFieldType::Json => ColumnType::Json,
        ParsedFieldType::DateTime => ColumnType::DateTime,
        ParsedFieldType::Enum(name) => ColumnType::Enum(name.to_string()),
        ParsedFieldType::Relation(_) => ColumnType::Integer,
    }
}

pub fn map_default(df: &ParsedFieldDefault) -> Option<ColumnDefault> {
    match df {
        ParsedFieldDefault::String(s) => Some(ColumnDefault::Value(DinocoValue::String(s.clone()))),
        ParsedFieldDefault::Integer(i) => Some(ColumnDefault::Value(DinocoValue::Integer(*i))),
        ParsedFieldDefault::Boolean(b) => Some(ColumnDefault::Value(DinocoValue::Boolean(*b))),
        ParsedFieldDefault::Function(func) => match func {
            FunctionCall::Now => Some(ColumnDefault::Function("NOW()".to_string())),
            FunctionCall::AutoIncrement | FunctionCall::Uuid | _ => None,
        },
        _ => None,
    }
}

pub fn map_referential_action(action: &Option<ReferentialAction>) -> Option<&'static str> {
    match action {
        Some(ReferentialAction::Cascade) => Some("CASCADE"),
        Some(ReferentialAction::SetNull) => Some("SET NULL"),
        Some(ReferentialAction::SetDefault) => Some("SET DEFAULT"),
        None => None,
    }
}
