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
    let inner_indent = base_indent.repeat(indent_level + 1);
    let outer_indent = base_indent.repeat(indent_level);

    match value {
        ConfigValue::String(s, _) => format!("\"{}\"", s),

        ConfigValue::Comment(span) => {
            format!("# {}", span.as_str().replace("#", "").trim())
        }

        ConfigValue::Array(arr, _) => {
            if arr.is_empty() {
                return "[]".to_string();
            }

            let mut out = String::from("[\n");

            let last_value_index = arr.iter().rposition(|v| !matches!(v, ConfigValue::Comment(_)));

            for i in 0..arr.len() {
                let item = &arr[i];
                let fmt = format_config_value(item, indent_level + 1, base_indent);

                match item {
                    ConfigValue::Comment(comment_span) => {
                        let is_inline = if i > 0 {
                            let prev_line = arr[i - 1].span().end_pos().line_col().0;
                            let curr_line = comment_span.start_pos().line_col().0;
                            prev_line == curr_line
                        } else {
                            false
                        };

                        if is_inline {
                            out.truncate(out.trim_end().len());
                            out.push_str(&format!(" {}\n", fmt));
                        } else {
                            out.push_str(&format!("{}{}\n", inner_indent, fmt));
                        }
                    }
                    _ => {
                        out.push_str(&format!("{}{}", inner_indent, fmt));

                        if Some(i) != last_value_index {
                            out.push(',');
                        }

                        out.push('\n');
                    }
                }
            }

            out.push_str(&format!("{}]", outer_indent));
            out
        }

        ConfigValue::Object(fields, _) => {
            if fields.is_empty() {
                return "{ }".to_string();
            }

            let mut parts = vec![];
            for f in fields {
                let val_str = if let Some(v) = &f.value {
                    format!(" = {}", format_config_value(v, indent_level + 1, base_indent))
                } else {
                    "".into()
                };
                parts.push(format!("{}{}{}", inner_indent, f.name, val_str));
            }
            format!("{{\n{}\n{}}}", parts.join("\n"), outer_indent)
        }

        ConfigValue::Function { name, args, .. } => {
            let args_str = args.iter().map(|v| format_config_value(v, indent_level, base_indent)).collect::<Vec<_>>().join(", ");
            format!("{}({})", name, args_str)
        }
    }
}
