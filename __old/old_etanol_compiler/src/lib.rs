#![allow(non_snake_case)]

use std::fs::read_to_string;

mod parsers;
mod tokenizer;
mod utils;

use parsers::Parser;
use tokenizer::Tokenizer;

pub use utils::{DatabaseConfig, ParsedToken, TableColumn};

pub struct Compiler {
    filename: String,
}

impl Compiler {
    pub fn new(filename: String) -> Self {
        Self { filename }
    }

    pub fn run(&self) -> Vec<ParsedToken> {
        let content =
            read_to_string(&self.filename).expect(&format!("file '{}' not found", self.filename));

        let tokenizer = Tokenizer::new(&self.filename, &content.trim());
        let mut parser = Parser::new(tokenizer);

        parser.run()
    }
}
