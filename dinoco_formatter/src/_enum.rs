use crate::FormatterConfig;
use crate::utils::get_capped_newlines;

use dinoco_compiler::Enum;

pub fn format_enum(_enum: &Enum, config: &FormatterConfig) -> String {
    let mut out = String::new();
    let indent = config.indent_str();

    out.push_str("enum ");
    out.push_str(&_enum.name);
    out.push_str(" {\n");

    for position in 0.._enum.total_blocks {
        if let Some((_, span)) = _enum.comments.iter().find(|x| x.0 == position) {
            let clean = span.as_str().replace('#', "");

            out.push_str(&indent);
            out.push_str("# ");
            out.push_str(clean.trim());

            let (line, _) = span.end_pos().line_col();

            if let Some(next_line) = get_next_start_line(_enum, position + 1) {
                out.push_str(&get_capped_newlines(line, next_line, true));
            } else {
                out.push('\n');
            }
        }

        if let Some((_, span)) = _enum.values.iter().find(|x| x.0 == position) {
            out.push_str(&indent);
            out.push_str(span.as_str());

            let (line, _) = span.end_pos().line_col();

            if let Some(next_line) = get_next_start_line(_enum, position + 1) {
                out.push_str(&get_capped_newlines(line, next_line, false));
            } else {
                out.push('\n');
            }
        }
    }

    out.push_str("}\n\n");
    out
}

fn get_next_start_line(_enum: &Enum, position: usize) -> Option<usize> {
    if let Some((_, span)) = _enum.comments.iter().find(|x| x.0 == position) {
        return Some(span.start_pos().line_col().0);
    }

    if let Some((_, span)) = _enum.values.iter().find(|x| x.0 == position) {
        return Some(span.start_pos().line_col().0);
    }

    None
}
