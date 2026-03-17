use crate::utils::DatabaseConfig;
use crate::utils::{readString, ParsedToken, Pointer, Token};

pub fn config(pointer: &mut Pointer) {
    if pointer
        .tokens
        .iter()
        .find(|x| match x {
            ParsedToken::DatabaseConfigs(..) => true,
            _ => false,
        })
        .is_some()
    {
        pointer.error("Duplicate config block");
    }

    pointer.take("Keyword");

    let startBracket = pointer.toEqual("Brackets", "{");
    if !startBracket {
        pointer.error(&format!("Expected a '{}'", "{"));
    }

    let mut configs: Vec<DatabaseConfig> = vec![];

    loop {
        if pointer.toEqual("Brackets", "}") {
            break;
        }

        match pointer.token.clone() {
            None | Some(Token::EOF) => {
                pointer.error(&format!("Expected a '{}'", "}"));
            }
            Some(Token::ConfigKey(identifier)) => {
                pointer.take("ConfigKey");

                let assign = pointer.toEqual("Punctuation", "=");
                if !assign {
                    pointer.error(&format!("Expected a '='"));
                }

                match pointer.token.clone() {
                    Some(Token::Punctuation(data)) if data == "\"" => {
                        let value = readString(pointer);

                        configs.push(DatabaseConfig::Value(identifier, value));

                        continue;
                    }
                    _ => {}
                }

                if pointer.toEqual("Function", "env") {
                    let startBracket = pointer.toEqual("Brackets", "(");
                    if !startBracket {
                        pointer.error(&format!("Expected a '{}'", "("));
                    }

                    let value = readString(pointer);

                    let endBracket = pointer.toEqual("Brackets", ")");
                    if !endBracket {
                        pointer.error(&format!("Expected a '{}'", ")"));
                    }

                    configs.push(DatabaseConfig::Env(identifier, value));

                    continue;
                }

                pointer.error(&format!("Expected a '\"' or a env function"));
            }
            Some(Token::Identifier(value)) => {
                pointer.error(&format!("Unexpected config key '{}'", value));
            }
            _ => break,
        }
    }

    if configs.len() < pointer.tokenizer.configs.len() {
        pointer.error("Missing configs");
    }

    pointer.tokens.push(ParsedToken::DatabaseConfigs(configs));
}

// table User {
//     id String
//     name String
//     age Integer?
// }
