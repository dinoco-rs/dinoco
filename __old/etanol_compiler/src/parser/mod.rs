mod config;
mod pointer;
mod table;

pub use pointer::Pointer;

use crate::{ParsedToken, Token, Tokenizer};

pub struct Parser {
    pub pointer: Pointer,
}

impl Parser {
    pub fn new(tokenizer: Tokenizer) -> Self {
        Self {
            pointer: Pointer::new(tokenizer),
        }
    }

    pub fn run(&mut self) {
        if self.pointer.token.is_none() {
            self.pointer.next();
        }

        loop {
            match self.pointer.token.clone() {
                Some(Token::EOF) | None => break,
                Some(Token::Keyword(key)) => match key.as_str() {
                    "config" => config::config(&mut self.pointer),
                    "table" => table::table(&mut self.pointer),
                    _ => self
                        .pointer
                        .error(format!("Error: Unexpected keyword: '{}'", key)),
                },

                _ => {}
            }
        }

        let config = self.pointer.tokens.iter().find(|x| match x {
            ParsedToken::Config(_) => true,
            _ => false,
        });

        if config.is_none() {
            self.pointer.error("Error: No config found".to_string());
        }

        for token in self.pointer.tokens.iter() {
            match token {
                ParsedToken::Table(name, columns) => {
                    println!("tableName: {:?}", name);

                    for column in columns {
                        println!("{:?}", column);
                    }

                    println!("\n");
                }
                ParsedToken::Config(config) => {
                    println!("config: ");

                    for conf in config.iter() {
                        println!("{:?}", conf);
                    }

                    println!("\n");
                }
            }
        }
    }
}
