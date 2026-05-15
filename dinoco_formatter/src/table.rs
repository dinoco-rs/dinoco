use std::collections::HashMap;

use crate::FormatterConfig;
use crate::utils::{format_newlines, get_capped_newlines};

use dinoco_compiler::{Field, FieldDefaultValue, FieldType, FunctionCall, Relation, Table};

pub fn format_table(table: &Table, config: &FormatterConfig) -> String {
    let mut out = String::new();
    let indent = config.indent_str();

    out.push_str(&format!("model {} {{\n", table.name));

    let grouped_widths = get_grouped_widths(table);

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
            let (max_name_len, max_type_len) =
                grouped_widths.get(&field.position).copied().unwrap_or((field.name.len(), type_str.len()));

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

            out = out.trim().to_string();

            out.push_str(&format_newlines(field.newlines, false));
        }
    }

    for decorator in get_model_decorators(table) {
        out.push_str(&indent);
        out.push_str(&decorator);
        out.push('\n');
    }

    out.push_str("}\n\n");
    out
}

fn get_grouped_widths(table: &Table) -> HashMap<usize, (usize, usize)> {
    let mut grouped_widths = HashMap::new();
    let mut ordered_fields = table.fields.iter().collect::<Vec<_>>();

    ordered_fields.sort_by_key(|field| field.position);

    let mut group = Vec::new();

    for (index, field) in ordered_fields.iter().enumerate() {
        group.push(*field);

        let closes_group = field.newlines > 1 || index + 1 == ordered_fields.len();

        if closes_group {
            let max_name_len = group.iter().map(|item| item.name.len()).max().unwrap_or_default();
            let max_type_len = group.iter().map(|item| get_full_type_string(item).len()).max().unwrap_or_default();

            for grouped_field in &group {
                grouped_widths.insert(grouped_field.position, (max_name_len, max_type_len));
            }

            group.clear();
        }
    }

    grouped_widths
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
        FieldType::Json => "Json".to_string(),
        FieldType::DateTime => "DateTime".to_string(),
        FieldType::Date => "Date".to_string(),
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

fn get_model_decorators(table: &Table) -> Vec<String> {
    let mut decorators = Vec::new();

    if let Some(mapped_name) = &table.mapped_name {
        decorators.push(format!("@@table_name(\"{}\")", mapped_name));
    }

    if !table.primary_key_fields.is_empty() {
        decorators.push(format!("@@ids([{}])", table.primary_key_fields.join(", ")));
    }

    for unique_set in &table.unique_field_sets {
        if !unique_set.is_empty() {
            decorators.push(format!("@@uniques([{}])", unique_set.join(", ")));
        }
    }

    for index_set in &table.index_field_sets {
        if !index_set.is_empty() {
            decorators.push(format!("@@indexes([{}])", index_set.join(", ")));
        }
    }

    decorators
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
            FunctionCall::Now => "now()".to_string(),
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
