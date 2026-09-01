#[derive(Debug)]
struct PreservedComment {
    text: String,
    token_index: usize,
    inline: bool,
}

/// Reattaches comments after formatting. The parser intentionally treats
/// comments as whitespace, so comments are anchored to the surrounding lexical
/// token instead of being stored in the schema AST.
pub fn restore(source: &str, formatted: &str) -> String {
    let comments = comments(source);
    if comments.is_empty() {
        return formatted.to_string();
    }

    let token_lines = token_lines(formatted);
    let mut before = std::collections::BTreeMap::<usize, Vec<String>>::new();
    let mut inline = std::collections::BTreeMap::<usize, Vec<String>>::new();
    let line_count = formatted.lines().count();

    for comment in comments {
        if comment.inline && comment.token_index > 0 {
            let line = token_lines.get(comment.token_index - 1).copied().unwrap_or(line_count.saturating_sub(1));
            inline.entry(line).or_default().push(comment.text);
        } else {
            let line = token_lines.get(comment.token_index).copied().unwrap_or(line_count);
            before.entry(line).or_default().push(comment.text);
        }
    }

    let had_final_newline = formatted.ends_with('\n');
    let lines = formatted.lines().collect::<Vec<_>>();
    let mut out = String::new();
    for line_index in 0..=lines.len() {
        if let Some(items) = before.get(&line_index) {
            let indent =
                lines.get(line_index).map(|line| &line[..line.len() - line.trim_start().len()]).unwrap_or_default();
            for comment in items {
                out.push_str(indent);
                out.push_str(comment);
                out.push('\n');
            }
        }
        let Some(line) = lines.get(line_index) else {
            continue;
        };
        out.push_str(line);
        if let Some(items) = inline.get(&line_index) {
            for comment in items {
                out.push_str("  ");
                out.push_str(comment);
            }
        }
        if line_index + 1 < lines.len() || had_final_newline {
            out.push('\n');
        }
    }
    out
}

fn comments(source: &str) -> Vec<PreservedComment> {
    let mut result = Vec::new();
    let mut token_index = 0usize;
    for line in source.lines() {
        let (tokens, comment) = scan_line(line);
        if let Some((text, inline)) = comment {
            result.push(PreservedComment { text, token_index: token_index + tokens, inline });
        }
        token_index += tokens;
    }
    result
}

fn token_lines(source: &str) -> Vec<usize> {
    let mut result = Vec::new();
    for (line_index, line) in source.lines().enumerate() {
        let (count, _) = scan_line(line);
        result.extend(std::iter::repeat_n(line_index, count));
    }
    result
}

fn scan_line(line: &str) -> (usize, Option<(String, bool)>) {
    let bytes = line.as_bytes();
    let mut index = 0usize;
    let mut tokens = 0usize;
    let mut has_code = false;

    while index < bytes.len() {
        match bytes[index] {
            b' ' | b'\t' | b'\r' => index += 1,
            b'#' => return (tokens, Some((line[index..].trim_end().to_string(), has_code))),
            b'/' if bytes.get(index + 1) == Some(&b'/') => {
                return (tokens, Some((line[index..].trim_end().to_string(), has_code)));
            }
            b'"' => {
                tokens += 1;
                has_code = true;
                index += 1;
                while index < bytes.len() {
                    if bytes[index] == b'\\' {
                        index = (index + 2).min(bytes.len());
                    } else if bytes[index] == b'"' {
                        index += 1;
                        break;
                    } else {
                        index += 1;
                    }
                }
            }
            byte if byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-' => {
                tokens += 1;
                has_code = true;
                index += 1;
                while index < bytes.len()
                    && (bytes[index].is_ascii_alphanumeric() || matches!(bytes[index], b'_' | b'-' | b'.'))
                {
                    index += 1;
                }
            }
            b',' | b':' | b'=' => {
                // These separators may be inserted by canonical multiline
                // formatting, so they are deliberately not comment anchors.
                has_code = true;
                index += 1;
            }
            _ => {
                tokens += 1;
                has_code = true;
                index += 1;
            }
        }
    }
    (tokens, None)
}
