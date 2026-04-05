use dinoco_compiler::{ConnectionUrl, ParsedField, ParsedFieldType};

pub(crate) fn render_connection_url(url: &ConnectionUrl) -> String {
    match url {
        ConnectionUrl::Literal(value) => format!("String::from({value:?})"),
        ConnectionUrl::Env(value) => {
            format!("std::env::var({value:?}).expect(\"missing environment variable for Dinoco client generation\")")
        }
    }
}

pub(crate) fn scalar_fields(fields: &[ParsedField]) -> Vec<&ParsedField> {
    fields.iter().filter(|field| !matches!(field.field_type, ParsedFieldType::Relation(_))).collect()
}

pub(crate) fn relation_fields(fields: &[ParsedField]) -> Vec<&ParsedField> {
    fields.iter().filter(|field| matches!(field.field_type, ParsedFieldType::Relation(_))).collect()
}

pub(crate) fn rust_scalar_type(field: &ParsedField, enum_names: &[String]) -> String {
    let base = match &field.field_type {
        ParsedFieldType::String => "String".to_string(),
        ParsedFieldType::Boolean => "bool".to_string(),
        ParsedFieldType::Integer => "i64".to_string(),
        ParsedFieldType::Float => "f64".to_string(),
        ParsedFieldType::Json => "dinoco::JsonValue".to_string(),
        ParsedFieldType::DateTime => "dinoco::DateTimeUtc<dinoco::Utc>".to_string(),
        ParsedFieldType::Date => "dinoco::NaiveDate".to_string(),
        ParsedFieldType::Enum(name) => {
            if enum_names.iter().any(|item| item == name) {
                format!("super::enums::{name}")
            } else {
                "String".to_string()
            }
        }
        ParsedFieldType::Relation(_) => unreachable!(),
    };

    if field.is_optional {
        format!("Option<{base}>")
    } else if field.is_list {
        format!("Vec<{base}>")
    } else {
        base
    }
}

pub(crate) fn filter_type(field: &ParsedField, enum_names: &[String]) -> String {
    match &field.field_type {
        ParsedFieldType::String => "String".to_string(),
        ParsedFieldType::Boolean => "bool".to_string(),
        ParsedFieldType::Integer => "i64".to_string(),
        ParsedFieldType::Float => "f64".to_string(),
        ParsedFieldType::Json => "dinoco::JsonValue".to_string(),
        ParsedFieldType::DateTime => "dinoco::DateTimeUtc<dinoco::Utc>".to_string(),
        ParsedFieldType::Date => "dinoco::NaiveDate".to_string(),
        ParsedFieldType::Enum(name) => {
            if enum_names.iter().any(|item| item == name) {
                format!("super::enums::{name}")
            } else {
                "String".to_string()
            }
        }
        ParsedFieldType::Relation(_) => unreachable!(),
    }
}

pub(crate) fn relation_target(field: &ParsedField) -> String {
    match &field.field_type {
        ParsedFieldType::Relation(name) => {
            let module_name = to_snake_case(name);

            format!("super::{module_name}::{name}")
        }
        _ => unreachable!(),
    }
}

pub(crate) fn to_snake_case(value: &str) -> String {
    let mut output = String::new();

    for (index, ch) in value.chars().enumerate() {
        if ch.is_uppercase() {
            if index > 0 {
                output.push('_');
            }

            output.extend(ch.to_lowercase());
        } else {
            output.push(ch);
        }
    }

    output
}

pub(crate) fn pascal_case(value: &str) -> String {
    let mut output = String::new();
    let mut uppercase_next = true;

    for ch in value.chars() {
        if ch == '_' || ch == '-' || ch == ' ' {
            uppercase_next = true;
            continue;
        }

        if uppercase_next {
            output.extend(ch.to_uppercase());
            uppercase_next = false;
        } else {
            output.push(ch);
        }
    }

    output
}
