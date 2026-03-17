mod config;
mod table;

use crate::{
    utils::{ParsedToken, Pointer, Token},
    Tokenizer,
};

pub struct Parser {
    pointer: Pointer,
}

impl Parser {
    pub fn new(tokenizer: Tokenizer) -> Self {
        Self {
            pointer: Pointer::new(tokenizer),
        }
    }

    pub fn run(&mut self) -> Vec<ParsedToken> {
        let mut pointer = self.pointer.clone();

        if pointer.token.is_none() {
            pointer.next();
        }

        loop {
            match pointer.token.clone() {
                None | Some(Token::EOF) => break,
                Some(Token::Keyword(keyword)) => match keyword.as_str() {
                    "config" => config::config(&mut pointer),
                    "table" => table::table(&mut pointer),
                    _ => pointer.error(&format!("Error: Unexpected '{}'", keyword)),
                },
                Some(data) => {
                    pointer.error(&format!("Error: Unexpected '{}'", data.tokenValue()));
                }
            }
        }

        let config = pointer.tokens.iter().find(|x| match x {
            ParsedToken::DatabaseConfigs(_) => true,
            _ => false,
        });

        if config.is_none() {
            pointer.error("Error: No config found");
        }

        return pointer.tokens;
    }
}
