use dinoco_compiler::{Config as AstConfig, ConfigValue};

use crate::FormatterConfig;
use crate::utils::get_capped_newlines;

pub fn format_config(ast_config: &AstConfig, config: &FormatterConfig) -> String {
    let mut out = String::new();
    let indent = config.indent_str();

    out.push_str("config {\n");

    let mut is_new_line = true;

    for position in 0..ast_config.total_fields {
        if let Some((_, span)) = ast_config.comments.iter().find(|x| x.0 == position) {
            let clean = span.as_str().replace('#', "");

            if is_new_line {
                out.push_str(&indent);
            }

            out.push_str("# ");
            out.push_str(clean.trim());

            let current_end = span.end_pos().line_col().0;

            if let Some((next_start, next_is_comment)) = get_next_info(ast_config, position + 1) {
                let nl = get_capped_newlines(current_end, next_start, next_is_comment);
                is_new_line = nl.contains('\n');
                out.push_str(&nl);
            } else {
                out.push('\n');
                is_new_line = true;
            }
        }

        if let Some(field) = ast_config.fields.iter().find(|x| x.position == position) {
            if is_new_line {
                out.push_str(&indent);
            }

            out.push_str(&field.name);

            if let Some(val) = &field.value {
                out.push_str(" = ");
                out.push_str(&format_config_value(val, 1, &indent));
            }

            for comment_span in &field.comments {
                let (field_line, _) = field.span.start_pos().line_col();
                let (comment_line, _) = comment_span.start_pos().line_col();

                let clean = comment_span.as_str().replace('#', "");

                if comment_line == field_line {
                    out.push_str(" # ");
                    out.push_str(clean.trim());
                } else {
                    out.push('\n');
                    out.push_str(&indent);
                    out.push_str("# ");
                    out.push_str(clean.trim());
                }
            }

            let current_end = field.span.end_pos().line_col().0;

            if let Some((next_start, next_is_comment)) = get_next_info(ast_config, position + 1) {
                let nl = get_capped_newlines(current_end, next_start, next_is_comment);
                is_new_line = nl.contains('\n');
                out.push_str(&nl);
            } else {
                out.push('\n');
                is_new_line = true;
            }
        }
    }

    out.push_str("}\n\n");
    out
}

fn get_next_info(ast_config: &AstConfig, start_search_pos: usize) -> Option<(usize, bool)> {
    for pos in start_search_pos..=ast_config.total_fields {
        if let Some((_, span)) = ast_config.comments.iter().find(|x| x.0 == pos) {
            return Some((span.start_pos().line_col().0, true));
        }

        if let Some(field) = ast_config.fields.iter().find(|x| x.position == pos) {
            return Some((field.span.start_pos().line_col().0, false));
        }
    }

    None
}

fn format_config_value(value: &ConfigValue, indent_level: usize, base_indent: &str) -> String {
    match value {
        ConfigValue::String(s) => format!("\"{}\"", s),

        ConfigValue::Array(arr) => match arr.len() {
            0 => "[]".to_string(),

            1 => {
                let item = format_config_value(&arr[0], indent_level, base_indent);
                format!("[{}]", item)
            }

            _ => {
                let inner_indent = base_indent.repeat(indent_level + 1);
                let outer_indent = base_indent.repeat(indent_level);

                let items: Vec<String> = arr
                    .iter()
                    .map(|v| format!("{}{}", inner_indent, format_config_value(v, indent_level + 1, base_indent)))
                    .collect();

                format!("[\n{}\n{}]", items.join(",\n"), outer_indent)
            }
        },

        ConfigValue::Function { name, args } => {
            let args_str = args.iter().map(|v| format_config_value(v, indent_level, base_indent)).collect::<Vec<_>>().join(", ");

            format!("{}({})", name, args_str)
        }

        ConfigValue::Object(_) => "{ }".to_string(),
    }
}
