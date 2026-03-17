use crate::{Column, DecoratorMethods, ParsedToken, Pointer, Token};

fn read_column(pointer: &mut Pointer, columns: &mut Vec<Column>) {
    let state = pointer.state();
    let name = pointer.take_with_error("Identifier", "Error: Expected column name");

    if pointer.tokenizer.line != state.line {
        pointer.error(format!(
            "Error: Expected '{}' in line {}",
            pointer.token.clone().unwrap().value(),
            state.line
        ));
    }

    let _type = pointer.take_with_error(
        "Type",
        &format!(
            "Error: Unexpected '{}'",
            pointer.token.clone().unwrap().value()
        ),
    );

    let is_optional = pointer.take_with_line_value("Punctuation", "?", state.line);

    let mut decorators = vec![];

    loop {
        if let Some(_) = pointer.take_with_line_value("Symbol", "@", state.line) {
            match pointer.token.clone() {
                Some(Token::Identifier(name)) => {
                    if !pointer.config.decorators.exists(name.clone()) {
                        pointer.error(format!("Error: Unknown decorator '{}'", name));
                    }

                    pointer.take_with_line("Identifier", state.line);
                    decorators.push(name);

                    continue;
                }
                _ => pointer.error(format!("Error: Unexpected '{}'", "@")),
            }
        }

        break;
    }

    let primary_key = decorators.contains(&"id".to_string());
    let autoincrement = decorators.contains(&"autoincrement".to_string());
    let uuid = decorators.contains(&"uuid".to_string());

    if autoincrement {
        if !primary_key {
            pointer.restore(state.clone());

            pointer.error(format!("Error: add @id"));
        }

        if uuid {
            pointer.restore(state.clone());

            pointer.error(format!("Error: remove @uuid"));
        }

        if _type.value() != "Number" {
            pointer.restore(state.clone());

            pointer.error(format!(
                "Error: change '{}' type from '{}' to '{}'",
                name.value(),
                _type.value(),
                "Number"
            ));
        }
    }

    columns.push(Column {
        name: name.value(),
        _type: _type.value(),
        is_optional: is_optional.is_some(),
        primary_key,
        autoincrement,
        uuid,
        default: None,
    });
}

pub fn table(pointer: &mut Pointer) {
    let state = pointer.state();

    pointer.take("Keyword");

    match pointer.token.clone() {
        Some(Token::Identifier(name)) => {
            let letter = name.get(..1).unwrap();
            if letter != letter.to_uppercase() {
                pointer.error(format!(
                    "Error: Table name must start with an uppercase letter: '{}'",
                    letter
                ));
            }

            pointer.take("Identifier");
            pointer.equal_with_error("Bracket", "{", "Error: Expected '{'");

            let mut columns: Vec<Column> = vec![];

            while !pointer.equal("Bracket", "}") {
                read_column(pointer, &mut columns);
            }

            let amount = columns.iter().filter(|x| x.primary_key).count();

            if amount > 1 {
                pointer.restore(state.clone());

                pointer.error("Error: Only one primary key is allowed".to_string());
            } else if amount == 0 {
                pointer.restore(state.clone());

                pointer.error("Error: Expected one column primary key".to_string());
            }

            let has_primary_key = columns.iter().find(|c| c.primary_key);
            if has_primary_key.is_none() {
                pointer.restore(state);

                pointer.error("Error: Table must have a primary key".to_string());
            }

            pointer.tokens.push(ParsedToken::Table(name, columns));
        }
        _ => pointer.error("Error: Expected identifier".to_string()),
    }
}
