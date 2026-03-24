use crate::FormatterConfig;
use crate::utils::{format_newlines, get_capped_newlines};

use dinoco_compiler::{Field, FieldDefaultValue, FieldType, FunctionCall, Relation, Table};

pub fn format_table(table: &Table, config: &FormatterConfig) -> String {
    let mut out = String::new();
    let indent = config.indent_str();

    out.push_str(&format!("model {} {{\n", table.name));

    let (max_name_len, max_type_len) = get_max_widths(table);

    for position in 0..table.total_fields {
        if let Some((_, span)) = table.comments.iter().find(|x| x.0 == position) {
            let clean = span.as_str().replace('#', "").trim().to_string();

            out.push_str(&indent);
            out.push_str("# ");
            out.push_str(&clean);

            let current_end = span.end_pos().line_col().0;

            if let Some((next_start, next_is_comment)) = get_next_info(table, position) {
                out.push_str(&get_capped_newlines(current_end, next_start, next_is_comment));
            } else {
                out.push('\n');
            }
        }

        if let Some(field) = table.fields.iter().find(|x| x.position == position) {
            let type_str = get_full_type_string(field);

            let name_padded = format!("{:<width$}", field.name, width = max_name_len);
            let type_padded = format!("{:<width$}", type_str, width = max_type_len);

            out.push_str(&indent);
            out.push_str(&name_padded);
            out.push_str("  ");
            out.push_str(&type_padded);

            let decorators = get_decorators_string(field);
            if !decorators.is_empty() {
                out.push_str("  ");
                out.push_str(&decorators);
            }

            for comment in &field.comments {
                let clean = comment.replace('#', "");
                out.push_str(" # ");
                out.push_str(clean.trim());
            }

            out.push_str(&format_newlines(field.newlines, false));
        }
    }

    out.push_str("}\n\n");
    out
}

fn get_max_widths(table: &Table) -> (usize, usize) {
    let mut max_name_len = 0;
    let mut max_type_len = 0;

    for field in &table.fields {
        max_name_len = max_name_len.max(field.name.len());
        max_type_len = max_type_len.max(get_full_type_string(field).len());
    }

    (max_name_len, max_type_len)
}

fn get_next_info(table: &Table, current_pos: usize) -> Option<(usize, bool)> {
    for pos in (current_pos + 1)..=table.total_fields {
        if let Some((_, span)) = table.comments.iter().find(|x| x.0 == pos) {
            return Some((span.start_pos().line_col().0, true));
        }

        if let Some(field) = table.fields.iter().find(|x| x.position == pos) {
            return Some((field.span.start_pos().line_col().0, false));
        }
    }

    None
}

fn get_full_type_string(field: &Field) -> String {
    let mut type_str = match &field.field_type {
        FieldType::String => "String".to_string(),
        FieldType::Boolean => "Boolean".to_string(),
        FieldType::Integer => "Integer".to_string(),
        FieldType::Float => "Float".to_string(),
        FieldType::Custom(c) => c.clone(),
    };

    if field.is_list {
        type_str.push_str("[]");
    }

    if field.is_optional {
        type_str.push('?');
    }

    type_str
}

fn get_decorators_string(field: &Field) -> String {
    let mut decorators = Vec::new();

    if field.is_primary_key {
        decorators.push("@id".to_string());
    }

    if field.is_unique {
        decorators.push("@unique".to_string());
    }

    if !matches!(field.default_value, FieldDefaultValue::NotDefined) {
        decorators.push(format!("@default({})", format_default_value(&field.default_value)));
    }

    if let Some(rel) = &field.relation {
        decorators.push(format_relation(rel));
    }

    decorators.join(" ")
}

fn format_default_value(dv: &FieldDefaultValue) -> String {
    match dv {
        FieldDefaultValue::NotDefined => String::new(),
        FieldDefaultValue::String(s) => format!("\"{}\"", s),
        FieldDefaultValue::Boolean(b) => b.to_string(),
        FieldDefaultValue::Integer(i) => i.to_string(),
        FieldDefaultValue::Float(f) => f.to_string(),
        FieldDefaultValue::Custom(c) => c.clone(),
        FieldDefaultValue::Function(f) => match f {
            FunctionCall::Uuid => "uuid()".to_string(),
            FunctionCall::Snowflake => "snowflake()".to_string(),
            FunctionCall::AutoIncrement => "autoincrement()".to_string(),
            FunctionCall::Env(p) => format!("env({})", p),
        },
    }
}

fn format_relation(rel: &Relation) -> String {
    let mut args = Vec::new();
    let keys = ["name", "fields", "references", "onUpdate", "onDelete"];

    for key in keys {
        if let Some(vals) = rel.named_params.get(key) {
            if key == "fields" || key == "references" {
                args.push(format!("{}: [{}]", key, vals.join(", ")));
            } else {
                args.push(format!("{}: {}", key, vals[0]));
            }
        }
    }

    format!("@relation({})", args.join(", "))
}
