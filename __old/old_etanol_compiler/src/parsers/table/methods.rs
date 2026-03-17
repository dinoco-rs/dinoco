use crate::utils::readString;
use crate::utils::{Pointer, TableColumn, Token};

pub fn readFunction(pointer: &mut Pointer) -> String {
    let startBracket = pointer.toEqual("Brackets", "(");
    if !startBracket {
        pointer.error("Expected a '('");
    }

    let mut result = String::new();

    match pointer.token.clone() {
        Some(Token::Identifier(value)) => {
            pointer.take("Identifier");

            result = value;
        }
        Some(Token::Punctuation(pun)) if pun == "\"" => {
            let value = readString(pointer);
            if value.is_empty() {
                pointer.error("Expected a value for the default");
            }

            result = value;
        }
        _ => pointer.error("Expected a default value"),
    }

    let endBracket = pointer.toEqual("Brackets", ")");
    if !endBracket {
        pointer.error("Expected a ')'");
    }

    result
}

pub fn readDecorator(
    pointer: &mut Pointer,
    columnType: String,
    columns: &Vec<TableColumn>,
    allDecorators: &Vec<(String, String)>,
) -> Option<(String, String)> {
    if let Some(Token::Decorator(name)) = pointer.token.clone() {
        if !pointer.tokenizer.decorators.contains(&name) {
            pointer.error(&format!("Unexpected '@{}'", name));
        }

        if allDecorators
            .iter()
            .find(|x| x.0 == name.as_str())
            .is_some()
        {
            pointer.error(&format!("Duplicate @{}", name));
        }

        match name.as_str() {
            "id" => {
                if columns.iter().find(|x| x.isPrimary).is_some() {
                    pointer.error("Duplicate primary key");
                }

                pointer.take("Decorator");

                return Some((name, "".to_string()));
            }

            "uuid" => {
                pointer.take("Decorator");

                return Some((name, "".to_string()));
            }
            "autoincrement" => {
                if columnType != "Integer" {
                    pointer.error("Autoincrement can only be used with integer columns");
                }

                pointer.take("Decorator");

                return Some((name, "".to_string()));
            }
            "default" => {
                pointer.take("Decorator");

                let value = readFunction(pointer);

                match columnType.as_str() {
                    "String" => {}
                    "Integer" => {
                        if value.parse::<i32>().is_err() {
                            pointer.error(&format!("Expected a integer found '{}'", value));
                        }
                    }
                    "Boolean" => {
                        if value.parse::<bool>().is_err() {
                            pointer.error(&format!("Expected a true or false found '{}'", value));
                        }
                    }
                    _ => {}
                }

                return Some((name, value));
            }
            _ => {}
        }
    }

    None
}
