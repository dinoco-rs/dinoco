use serde_derive::Serialize;
use serde_json::to_string_pretty;

use crate::{
    utils::{ParsedToken, Token},
    Tokenizer,
};

#[derive(Clone)]
pub struct Pointer {
    pub tokenizer: Tokenizer,
    pub token: Option<Token>,

    pub tokens: Vec<ParsedToken>,
}

#[derive(Serialize)]
struct Error {
    line: usize,
    lineContent: String,
    reason: String,
    filename: String,
}

impl Pointer {
    pub fn new(tokenizer: Tokenizer) -> Self {
        Self {
            tokenizer,
            token: None,

            tokens: vec![],
        }
    }

    pub fn next(&mut self) {
        match self.tokenizer.getToken() {
            None | Some(Token::EOF) => {
                self.token = None;
            }
            Some(Token::Whitespace()) => self.next(),
            Some(Token::Identifier(data)) if data.len() <= 0 => self.next(),
            data => {
                self.token = data.clone();
            }
        }
    }

    pub fn toEqual(&mut self, r#type: &str, value: &str) -> bool {
        if let Some(token) = self.token.clone() {
            if token.tokenType() == r#type.to_string() && token.tokenValue() == value.to_string() {
                self.take(r#type);

                return true;
            }
        }

        false
    }

    pub fn previewNext(&mut self, skip: bool) -> Option<Token> {
        return self.tokenizer.previewNextToken(skip);
    }

    pub fn take(&mut self, r#type: &str) -> Option<Token> {
        if let Some(token) = self.token.clone() {
            if token.tokenType() == r#type.to_string() {
                self.next();

                return Some(token);
            }
        }

        None
    }

    pub fn error(&mut self, err: &str) {
        let (line, lineContent) = self.getLine(self.tokenizer.line);

        let error = Error {
            filename: self.tokenizer.filename.clone(),
            reason: err.to_string(),
            line,
            lineContent,
        };

        println!("{}", to_string_pretty(&error).unwrap());

        std::process::exit(1);
    }

    fn getLine(&self, line: usize) -> (usize, String) {
        let mut newLine = line;

        loop {
            if let Some(lineContent) = self.tokenizer.lines.get(newLine - 1) {
                return (newLine, lineContent.join("").replace("\n", ""));
            }

            newLine -= 1;
        }
    }
}
