use std::collections::HashMap;

use dinoco_compiler::Schema;

#[derive(Debug, Default)]
pub struct LayoutHints {
    model_blank_lines: HashMap<String, Vec<bool>>,
}

impl LayoutHints {
    pub fn from_source(source: &str, schema: &Schema) -> Self {
        let lines = source.lines().collect::<Vec<_>>();
        let mut model_blank_lines = HashMap::new();

        for model in schema.models() {
            let Some((body_start, body_end)) = model_body(&lines, &model.name) else {
                continue;
            };
            let mut hints = Vec::with_capacity(model.fields.len());
            let mut cursor = body_start;
            let mut previous_field_line = None;

            for field in &model.fields {
                let field_line = (cursor..body_end).find(|line| leading_identifier(lines[*line]) == Some(&field.name));
                let Some(field_line) = field_line else {
                    hints.push(false);
                    continue;
                };
                let has_blank_line = previous_field_line
                    .is_some_and(|previous| lines[previous + 1..field_line].iter().any(|line| line.trim().is_empty()));
                hints.push(has_blank_line);
                previous_field_line = Some(field_line);
                cursor = field_line + 1;
            }

            model_blank_lines.insert(model.name.clone(), hints);
        }

        Self { model_blank_lines }
    }

    pub fn model_blank_lines(&self, model: &str) -> Option<&[bool]> {
        self.model_blank_lines.get(model).map(Vec::as_slice)
    }
}

fn model_body(lines: &[&str], model_name: &str) -> Option<(usize, usize)> {
    let declaration = lines.iter().position(|line| declares_model(line, model_name))?;
    let mut depth = 0i32;
    let mut opened = false;
    let mut body_start = declaration + 1;

    for (line_index, line) in lines.iter().enumerate().skip(declaration) {
        let (opens, closes) = brace_counts(line);
        if !opened && opens > 0 {
            opened = true;
            body_start = line_index + 1;
        }
        if opened {
            depth += opens as i32;
            depth -= closes as i32;
            if depth == 0 {
                return Some((body_start, line_index));
            }
        }
    }

    None
}

fn declares_model(line: &str, model_name: &str) -> bool {
    let mut identifiers = line_identifiers(line);
    identifiers.next() == Some("model") && identifiers.next() == Some(model_name)
}

fn leading_identifier(line: &str) -> Option<&str> {
    let line = line.trim_start();
    if line.starts_with('#') || line.starts_with("//") {
        return None;
    }
    let length = line.chars().take_while(|character| character.is_ascii_alphanumeric() || *character == '_').count();
    (length > 0).then(|| &line[..length])
}

fn line_identifiers(line: &str) -> impl Iterator<Item = &str> {
    line.split(|character: char| !(character.is_ascii_alphanumeric() || character == '_'))
        .filter(|value| !value.is_empty())
}

fn brace_counts(line: &str) -> (usize, usize) {
    let mut opens = 0;
    let mut closes = 0;
    let mut quoted = false;
    let mut escaped = false;
    let mut characters = line.chars().peekable();

    while let Some(character) = characters.next() {
        if !quoted && (character == '#' || (character == '/' && characters.peek() == Some(&'/'))) {
            break;
        }
        if character == '"' && !escaped {
            quoted = !quoted;
        } else if !quoted {
            opens += usize::from(character == '{');
            closes += usize::from(character == '}');
        }
        escaped = quoted && character == '\\' && !escaped;
        if character != '\\' {
            escaped = false;
        }
    }

    (opens, closes)
}
