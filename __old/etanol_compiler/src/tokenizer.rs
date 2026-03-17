use diacritics::remove_diacritics;
use regex::Regex;

use crate::{Config, Token};

#[derive(Clone)]
pub struct State {
    pub line: usize,
    pub cursor: usize,
    pub letter: String,
}

pub struct Tokenizer {
    pub filename: String,
    pub content: String,
    pub lines: Vec<Vec<String>>,
    pub line: usize,

    cursor: usize,

    letter: String,
    config: Config,
}

impl Tokenizer {
    pub fn new(filename: String, content: String) -> Self {
        Self {
            filename,
            line: 0,
            cursor: 0,
            letter: String::new(),
            content: content.clone(),
            config: Config::new(),
            lines: content
                .lines()
                .map(|item| {
                    let mut result = String::from(item)
                        .split("")
                        .map(String::from)
                        .collect::<Vec<String>>();

                    result.pop();
                    result.remove(0);

                    result.push("\n".to_string());

                    return result;
                })
                .collect(),
        }
    }

    pub fn state(&mut self) -> State {
        State {
            line: self.line,
            cursor: self.cursor,
            letter: self.letter.clone(),
        }
    }

    pub fn restore(&mut self, data: State) {
        self.line = data.line;
        self.cursor = data.cursor;
        self.letter = data.letter;
    }

    pub fn next(&mut self) {
        self.cursor += 1;

        self.change_letter();
    }

    pub fn newline(&mut self) {
        self.line += 1;
        self.cursor = 0;

        self.change_letter();
    }

    pub fn preview(&mut self, skip: bool) -> Option<Token> {
        let (line, letter, cursor, mut token) = (
            self.line.clone(),
            self.letter.clone(),
            self.cursor.clone(),
            self.token(),
        );

        loop {
            match token {
                Some(Token::Whitespace) if skip => {
                    token = self.token();
                }
                _ => break,
            }
        }

        self.line = line;
        self.letter = letter;
        self.cursor = cursor;

        token
    }

    pub fn token(&mut self) -> Option<Token> {
        let mut _token: Option<Token> = None;

        if self.line == 0 && self.cursor == 0 {
            self.change_letter();
        }

        let letter = self.letter.clone();

        _token = match letter.as_str() {
            "EOF" => Some(Token::EOF),
            "{" | "}" | "(" | ")" => Some(Token::Bracket(letter)),
            "?" | "=" | "\"" | ";" | "+" | "-" | "/" | "." => Some(Token::Punctuation(letter)),
            "\n" => {
                self.newline();

                self.change_letter();

                self.token()
            }
            " " => Some(Token::Whitespace),
            "@" | ":" | "&" => Some(Token::Symbol(letter)),
            _ => {
                let mut word = String::new();

                loop {
                    match self.letter.as_str() {
                        "EOF" | " " | "\n" => break,
                        _ => {
                            if !self.is_letter() {
                                break;
                            }

                            word.push_str(&self.letter);
                        }
                    }

                    self.next();
                }

                let Config {
                    functions,
                    keywords,
                    types,
                    ..
                } = &self.config;

                if keywords.contains(&word) {
                    return Some(Token::Keyword(word));
                }

                if functions.contains(&word) {
                    return Some(Token::Function(word));
                }

                if types.contains(&word) {
                    return Some(Token::Type(word));
                }

                return Some(Token::Identifier(word));
            }
        };

        match _token {
            Some(Token::Identifier(_)) | Some(Token::Keyword(_)) | Some(Token::Function(_)) => {
                return _token;
            }
            _ => {
                self.next();

                return _token;
            }
        }
    }

    fn is_letter(&self) -> bool {
        let regex = Regex::new("[a-zA-Z0-9_]").unwrap();

        regex.is_match(&remove_diacritics(&self.letter))
    }

    fn change_letter(&mut self) {
        if let Some(line) = self.lines.get(self.line) {
            if let Some(letter) = line.get(self.cursor) {
                self.letter = letter.to_string();

                return;
            }
        }

        if self.line > self.lines.len() {
            self.letter = "EOF".to_string();
        }
    }
}
