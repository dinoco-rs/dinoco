use crate::read_string;
use crate::{DatabaseConfig, DatabaseConfigValue, ParsedToken, Pointer, Token};

pub fn config(pointer: &mut Pointer) {
    pointer.take("Keyword");
    pointer.duplicate_config();
    pointer.equal_with_error("Bracket", "{", "Error: Expected a '{'");

    let mut keys: Vec<DatabaseConfig> = vec![];

    while !pointer.equal("Bracket", "}") {
        match pointer.token.clone() {
            None | Some(Token::EOF) => pointer.error("Error: Expected a '}'".to_string()),
            Some(Token::Identifier(name)) => {
                if pointer.config.config_keys.contains(&name) {
                    pointer.take("Identifier");
                    pointer.equal_with_error("Punctuation", "=", "Expected a '='");

                    match pointer.token.clone() {
                        Some(Token::Punctuation(punc)) if punc == "\"" => {
                            keys.push(DatabaseConfig(
                                name,
                                DatabaseConfigValue::Value(read_string(pointer)),
                            ));
                        }
                        Some(Token::Function(func_name)) => {
                            if func_name == "env" {
                                pointer.take("Function");
                                pointer.equal_with_error("Bracket", "(", "Expected a '('");

                                let content = read_string(pointer);

                                pointer.equal_with_error("Bracket", ")", "Expected a ')'");

                                keys.push(DatabaseConfig(name, DatabaseConfigValue::Env(content)));

                                continue;
                            }

                            pointer.error(format!("Error: Unknown config function {}", name));
                        }
                        _ => pointer
                            .error("Error: Expected a \"value\" or env(VARIABLE_NAME)".to_string()),
                    }

                    continue;
                }

                pointer.error(format!("Error: Unknown config key {}", name));
            }
            _ => pointer.error("Error: Expected a \"value\" or env(VARIABLE_NAME)".to_string()),
        }
    }

    pointer.tokens.push(ParsedToken::Config(keys));
}
