#[derive(Debug)]
pub struct DatabaseConfig(pub String, pub DatabaseConfigValue);

#[derive(Debug)]
pub enum DatabaseConfigValue {
    Env(String),
    Value(String),
}

#[derive(Debug)]
pub struct Column {
    pub name: String,
    pub _type: String,
    pub default: Option<String>,
    pub uuid: bool,
    pub autoincrement: bool,
    pub is_optional: bool,
    pub primary_key: bool,
}

#[derive(Debug, Clone)]
pub enum Token {
    EOF,
    Whitespace,
    Identifier(String),
    Bracket(String),
    Punctuation(String),
    Symbol(String),
    Function(String),
    Keyword(String),
    Type(String),
}

#[derive(Debug)]
pub enum ParsedToken {
    Table(String, Vec<Column>),
    Config(Vec<DatabaseConfig>),
}

impl Token {
    pub fn _type(&self) -> String {
        match self {
            Token::Identifier(_) => "Identifier".to_string(),
            Token::Whitespace => "Whitespace".to_string(),
            Token::Keyword(_) => "Keyword".to_string(),
            Token::Punctuation(_) => "Punctuation".to_string(),
            Token::Bracket(_) => "Bracket".to_string(),
            Token::Symbol(_) => "Symbol".to_string(),
            Token::Function(_) => "Function".to_string(),
            Token::Type(_) => "Type".to_string(),
            Token::EOF => "EOF".to_string(),
        }
    }

    pub fn value(&self) -> String {
        match self {
            Token::Whitespace => " ".to_string(),
            Token::Identifier(data) => data.to_string(),
            Token::Keyword(data) => data.to_string(),
            Token::Punctuation(data) => data.to_string(),
            Token::Symbol(data) => data.to_string(),
            Token::Function(data) => data.to_string(),
            Token::Bracket(data) => data.to_string(),
            Token::Type(data) => data.to_string(),
            Token::EOF => "EOF".to_string(),
        }
    }
}
