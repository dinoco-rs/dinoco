use dinoco_compiler::{AttributeArgument, AttributeValue, ConfigValue};

use crate::FormatterConfig;

pub fn format_config_value(value: &ConfigValue, config: &FormatterConfig, indent_level: usize) -> String {
    match value {
        ConfigValue::String(value) => format!("\"{}\"", escape_string(value)),
        ConfigValue::Env(value) => format!("env(\"{}\")", escape_string(value)),
        ConfigValue::Boolean(value) => value.to_string(),
        ConfigValue::Integer(value) => value.to_string(),
        ConfigValue::Ident(value) => value.clone(),
        ConfigValue::Array(values) => format_config_array(values, config, indent_level),
        ConfigValue::Object(entries) => format_config_object(entries, config, indent_level),
    }
}

fn format_config_object(
    entries: &[dinoco_compiler::ConfigEntry],
    config: &FormatterConfig,
    indent_level: usize,
) -> String {
    let inner_indent = config.indent().repeat(indent_level + 1);
    let outer_indent = config.indent().repeat(indent_level);
    let mut out = String::from("{\n");
    for entry in entries {
        out.push_str(&inner_indent);
        out.push_str(&entry.key);
        out.push_str(" = ");
        out.push_str(&format_config_value(&entry.value, config, indent_level + 1));
        out.push('\n');
    }
    out.push_str(&outer_indent);
    out.push('}');
    out
}

pub fn format_attribute_value(value: &AttributeValue) -> String {
    match value {
        AttributeValue::String(value) => format!("\"{}\"", escape_string(value)),
        AttributeValue::Ident(value) => value.clone(),
        AttributeValue::Array(values) => {
            let values = values.iter().map(format_attribute_value).collect::<Vec<_>>();
            format!("[{}]", values.join(", "))
        }
        AttributeValue::Call { name, arguments } => {
            let arguments = arguments.iter().map(format_attribute_argument).collect::<Vec<_>>();
            format!("{name}({})", arguments.join(", "))
        }
    }
}

pub fn format_attribute_argument(argument: &AttributeArgument) -> String {
    match argument {
        AttributeArgument::Named { key, value } => format!("{key}: {}", format_attribute_value(value)),
        AttributeArgument::Value(value) => format_attribute_value(value),
    }
}

fn format_config_array(values: &[ConfigValue], config: &FormatterConfig, indent_level: usize) -> String {
    if values.is_empty() {
        return "[]".to_string();
    }

    let indent = config.indent();
    let inner_indent = indent.repeat(indent_level + 1);
    let outer_indent = indent.repeat(indent_level);
    let mut out = String::from("[\n");

    for (index, value) in values.iter().enumerate() {
        out.push_str(&inner_indent);
        out.push_str(&format_config_value(value, config, indent_level + 1));

        if index + 1 != values.len() {
            out.push(',');
        }

        out.push('\n');
    }

    out.push_str(&outer_indent);
    out.push(']');
    out
}

fn escape_string(value: &str) -> String {
    value
        .chars()
        .flat_map(|ch| match ch {
            '"' => "\\\"".chars().collect::<Vec<_>>(),
            '\\' => "\\\\".chars().collect::<Vec<_>>(),
            '\n' => "\\n".chars().collect::<Vec<_>>(),
            '\r' => "\\r".chars().collect::<Vec<_>>(),
            '\t' => "\\t".chars().collect::<Vec<_>>(),
            ch => vec![ch],
        })
        .collect()
}
