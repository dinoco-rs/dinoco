use crate::utils::Token;

use diacritics::remove_diacritics;
use regex::Regex;

#[derive(Debug, Clone)]
pub struct Tokenizer {
    pub filename: String,
    pub content: String,

    pub lines: Vec<Vec<String>>,
    pub line: usize,
    pub tableTypes: Vec<String>,
    pub configs: Vec<String>,
    pub decorators: Vec<String>,
    pub functions: Vec<String>,

    cursor: usize,
    letter: String,
    keywords: Vec<String>,
}

fn toString(data: Vec<&str>) -> Vec<String> {
    data.iter().map(|x| x.to_string()).collect::<Vec<_>>()
}

fn toLines(content: &str) -> Vec<Vec<String>> {
    content
        .split("\n")
        .map(|x| x.to_string())
        .map(|x| x.split("").map(|y| y.to_string()).collect::<Vec<String>>())
        .map(|mut l| {
            l.pop();
            l.remove(0);

            l.push("\n".to_string());

            return l;
        })
        .collect::<Vec<_>>()
}

impl Tokenizer {
    pub fn new(filename: &str, content: &str) -> Tokenizer {
        let lines = toLines(content);

        Self {
            filename: filename.to_string(),
            content: content.to_string(),
            lines,
            cursor: 0,
            line: 1,
            letter: String::new(),
            decorators: toString(vec!["id", "default", "uuid", "autoincrement"]),
            keywords: toString(vec!["table", "config"]),
            functions: toString(vec!["env"]),
            configs: toString(vec!["database", "database_url"]),
            tableTypes: toString(vec!["String", "Integer", "Boolean"]),
        }
    }

    fn isLetter(&self) -> bool {
        let regex = Regex::new("[a-zA-Z0-9_]").unwrap();

        regex.is_match(&remove_diacritics(&self.letter))
    }

    fn changeLetter(&mut self) {
        if let Some(contentLine) = self.lines.get(self.line - 1) {
            if let Some(letter) = contentLine.get(self.cursor) {
                self.letter = letter.to_string();

                return;
            }
        }

        if self.line > self.lines.len() {
            self.letter = "EOF".to_string();

            return;
        }
    }

    pub fn next(&mut self) {
        self.cursor += 1;

        self.changeLetter();
    }

    pub fn newline(&mut self) {
        self.cursor = 0;
        self.line += 1;

        self.changeLetter();
    }

    pub fn previewNextToken(&mut self, skip: bool) -> Option<Token> {
        let line = self.line.clone();
        let letter = self.letter.clone();
        let cursor = self.cursor.clone();

        let mut token = self.getToken();

        loop {
            match token {
                Some(Token::Whitespace()) if skip => {
                    token = self.getToken();
                }
                _ => break,
            }
        }

        self.line = line;
        self.cursor = cursor;
        self.letter = letter;

        return token;
    }

    pub fn getToken(&mut self) -> Option<Token> {
        let mut _token: Option<Token> = None;

        if self.line == 1 && self.cursor == 0 {
            self.changeLetter();
        }

        let letter = self.letter.clone();

        match letter.as_str() {
            "EOF" => _token = Some(Token::EOF),
            "{" | "}" | "(" | ")" => _token = Some(Token::Brackets(letter)),
            "?" | "=" | "\"" | ";" | "+" | "-" | "/" | "." => {
                _token = Some(Token::Punctuation(letter))
            }
            "\n" | "\\s" => {
                self.newline();

                self.changeLetter();

                return self.getToken();
            }
            " " => _token = Some(Token::Whitespace()),
            "@" | ":" | "&" => match letter.as_str() {
                "@" => {
                    self.next();

                    match self.previewNextToken(false) {
                        Some(Token::Identifier(identifier)) => {
                            if self.decorators.contains(&identifier) {
                                self.getToken();

                                return Some(Token::Decorator(identifier));
                            }
                        }
                        _ => {}
                    }

                    return Some(Token::Symbol(letter));
                }
                _ => _token = Some(Token::Symbol(letter)),
            },
            _ => {
                let mut word = String::new();

                loop {
                    match self.letter.as_str() {
                        "EOF" | " " | "\n" => break,
                        _ => {
                            if self.isLetter() {
                                word.push_str(&self.letter);
                            } else {
                                break;
                            }
                        }
                    }

                    self.next();
                }

                if self.keywords.contains(&word) {
                    return Some(Token::Keyword(word));
                }

                if self.tableTypes.contains(&word) {
                    return Some(Token::TableType(word));
                }

                if self.functions.contains(&word) {
                    return Some(Token::Function(word));
                }

                if self.configs.contains(&word) {
                    return Some(Token::ConfigKey(word));
                }

                return Some(Token::Identifier(word));
            }
        }

        self.next();

        _token
    }
}
