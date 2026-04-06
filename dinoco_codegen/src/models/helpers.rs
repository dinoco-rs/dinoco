use dinoco_compiler::{ConnectionUrl, FunctionCall, ParsedField, ParsedFieldDefault, ParsedFieldType};

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
    let base = rust_scalar_base_type(field, enum_names);

    if field.is_optional {
        format!("Option<{base}>")
    } else if field.is_list {
        format!("Vec<{base}>")
    } else {
        base
    }
}

pub(crate) fn rust_scalar_base_type(field: &ParsedField, enum_names: &[String]) -> String {
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

pub(crate) fn enum_variant_name(value: &str) -> String {
    if is_rust_keyword(value) {
        return format!("r#{value}");
    }

    value.to_string()
}

pub(crate) fn default_value_expr(field: &ParsedField, enum_names: &[String]) -> String {
    if field.is_optional {
        return "None".to_string();
    }

    if field.is_list {
        return "Vec::new()".to_string();
    }

    match &field.default_value {
        ParsedFieldDefault::String(value) => format!("{value:?}.to_string()"),
        ParsedFieldDefault::Boolean(value) => value.to_string(),
        ParsedFieldDefault::Integer(value) => value.to_string(),
        ParsedFieldDefault::Float(value) => value.to_string(),
        ParsedFieldDefault::EnumValue(value) => render_enum_expr(field, value, enum_names),
        ParsedFieldDefault::Function(function) => match function {
            FunctionCall::Uuid => "dinoco::uuid_v7().to_string()".to_string(),
            FunctionCall::Snowflake => "dinoco::snowflake()".to_string(),
            FunctionCall::AutoIncrement => "0".to_string(),
            FunctionCall::Now => match field.field_type {
                ParsedFieldType::Date => "dinoco::Utc::now().date_naive()".to_string(),
                _ => "dinoco::Utc::now()".to_string(),
            },
            FunctionCall::Env(_) => default_expr_by_type(field, enum_names),
        },
        ParsedFieldDefault::NotDefined => default_expr_by_type(field, enum_names),
    }
}

fn default_expr_by_type(field: &ParsedField, enum_names: &[String]) -> String {
    match &field.field_type {
        ParsedFieldType::String => "String::new()".to_string(),
        ParsedFieldType::Boolean => "false".to_string(),
        ParsedFieldType::Integer => "0".to_string(),
        ParsedFieldType::Float => "0.0".to_string(),
        ParsedFieldType::Json => "dinoco::JsonValue::Null".to_string(),
        ParsedFieldType::DateTime => "dinoco::Utc::now()".to_string(),
        ParsedFieldType::Date => "dinoco::Utc::now().date_naive()".to_string(),
        ParsedFieldType::Enum(name) => {
            if enum_names.iter().any(|item| item == name) {
                format!("<super::enums::{name} as Default>::default()")
            } else {
                "String::new()".to_string()
            }
        }
        ParsedFieldType::Relation(_) => unreachable!(),
    }
}

fn render_enum_expr(field: &ParsedField, value: &str, enum_names: &[String]) -> String {
    match &field.field_type {
        ParsedFieldType::Enum(name) if enum_names.iter().any(|item| item == name) => {
            format!("super::enums::{name}::{}", enum_variant_name(value))
        }
        _ => format!("{value:?}.to_string()"),
    }
}

fn is_rust_keyword(value: &str) -> bool {
    matches!(
        value,
        "as"
            | "break"
            | "const"
            | "continue"
            | "crate"
            | "else"
            | "enum"
            | "extern"
            | "false"
            | "fn"
            | "for"
            | "if"
            | "impl"
            | "in"
            | "let"
            | "loop"
            | "match"
            | "mod"
            | "move"
            | "mut"
            | "pub"
            | "ref"
            | "return"
            | "self"
            | "Self"
            | "static"
            | "struct"
            | "super"
            | "trait"
            | "true"
            | "type"
            | "unsafe"
            | "use"
            | "where"
            | "while"
            | "async"
            | "await"
            | "dyn"
            | "abstract"
            | "become"
            | "box"
            | "do"
            | "final"
            | "macro"
            | "override"
            | "priv"
            | "try"
            | "typeof"
            | "unsized"
            | "virtual"
            | "yield"
    )
}
