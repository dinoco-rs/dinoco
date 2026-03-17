use crate::{Config, ParsedToken, State, Token, Tokenizer};

pub struct Pointer {
    pub tokenizer: Tokenizer,
    pub token: Option<Token>,
    pub config: Config,

    pub tokens: Vec<ParsedToken>,
}

impl Pointer {
    pub fn new(tokenizer: Tokenizer) -> Self {
        Self {
            tokenizer,
            token: None,
            config: Config::new(),
            tokens: vec![],
        }
    }

    pub fn next(&mut self) {
        match self.tokenizer.token() {
            None | Some(Token::EOF) => self.token = None,
            Some(Token::Whitespace) => self.next(),
            Some(Token::Identifier(data)) if data.len() <= 0 => self.next(),
            data => self.token = data,
        }
    }

    pub fn equal(&mut self, _type: &str, value: &str) -> bool {
        if let Some(ref token) = self.token {
            if token._type() == _type && token.value() == value {
                self.take(_type);

                return true;
            }
        }

        false
    }
    pub fn equal_with_error(&mut self, _type: &str, value: &str, error: &str) {
        if let Some(token) = self.token.clone() {
            if token._type() == _type && token.value() == value {
                self.take(_type);

                return;
            }
        }

        self.error(error.to_string());
    }

    pub fn take_with_line(&mut self, r#type: &str, line: usize) -> Option<Token> {
        if let Some(token) = self.token.clone() {
            if token._type() == r#type {
                if self.tokenizer.line != line {
                    self.error(format!("Error: Unexpected '{}'", token.value()));
                }

                self.take(r#type);

                return Some(token);
            }
        }

        None
    }

    pub fn take_with_line_value(
        &mut self,
        r#type: &str,
        value: &str,
        line: usize,
    ) -> Option<Token> {
        if let Some(token) = self.token.clone() {
            if token._type() == r#type.to_string() && token.value() == value {
                if self.tokenizer.line != line {
                    self.error(format!(
                        "Error: Expected  '{}' in line {}",
                        token.value(),
                        line
                    ));
                }

                self.take(r#type);

                return Some(token);
            }
        }

        None
    }

    pub fn take(&mut self, r#type: &str) -> Option<Token> {
        if let Some(token) = self.token.clone() {
            if token._type() == r#type.to_string() {
                self.next();

                return Some(token);
            }
        }

        None
    }
    pub fn take_with_error(&mut self, r#type: &str, err: &str) -> Token {
        if let Some(token) = self.token.clone() {
            if token._type() == r#type.to_string() {
                self.next();

                return token;
            }
        }

        self.error(err.to_string());

        unreachable!()
    }

    pub fn take_with_value(&mut self, r#type: &str, value: &str) -> Option<Token> {
        if let Some(token) = self.token.clone() {
            if token._type() == r#type.to_string() && token.value() == value {
                self.next();

                return Some(token);
            }
        }

        None
    }

    pub fn error(&mut self, err: String) {
        println!("{}", err);
        println!(
            "Line {}: \"{}\"",
            self.tokenizer.line + 1,
            self.tokenizer
                .lines
                .get(self.tokenizer.line)
                .unwrap()
                .join("")
                .trim_end()
        );

        std::process::exit(1);
    }

    pub fn duplicate_config(&mut self) {
        let config = self.tokens.iter().find(|x| match x {
            ParsedToken::Config(_) => true,
            _ => false,
        });

        if config.is_some() {
            self.error("Error: Duplicate config block".to_string());
        }
    }

    pub fn state(&mut self) -> State {
        self.tokenizer.state()
    }

    pub fn restore(&mut self, data: State) {
        self.tokenizer.restore(data);
    }
}
