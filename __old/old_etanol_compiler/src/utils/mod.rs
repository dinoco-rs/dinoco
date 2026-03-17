mod methods;
mod pointer;

pub use methods::*;
pub use pointer::Pointer;

#[derive(Debug, Clone)]
pub enum Token {
    Identifier(String),
    Whitespace(),
    Keyword(String),
    Punctuation(String),
    Brackets(String),
    Symbol(String),
    Function(String),
    TableType(String),

    ConfigKey(String),
    Decorator(String),
    EOF,
}

#[derive(Debug, Clone)]
pub struct TableColumn {
    pub name: String,
    pub columnType: String,
    pub isOptional: bool,
    pub isPrimary: bool,
    pub uuid: bool,
    pub autoincrement: bool,
    pub default: Option<String>,
}

#[derive(Debug, Clone)]
pub enum DatabaseConfig {
    Env(String, String),
    Value(String, String),
}

#[derive(Debug, Clone)]
pub enum ParsedToken {
    DatabaseConfigs(Vec<DatabaseConfig>),
    Table(String, Vec<TableColumn>),
}

impl DatabaseConfig {
    pub fn key(&self) -> String {
        match self {
            DatabaseConfig::Env(key, _) => key.to_string(),
            DatabaseConfig::Value(key, _) => key.to_string(),
        }
    }
}

impl ParsedToken {
    pub fn tokenType(&self) -> String {
        match self {
            ParsedToken::DatabaseConfigs(..) => "DatabaseConfigs".to_string(),
            ParsedToken::Table(..) => "Table".to_string(),
        }
    }

    pub fn tokenValue(&self) -> Option<Vec<DatabaseConfig>> {
        match self {
            ParsedToken::DatabaseConfigs(configs) => Some(configs.clone()),
            ParsedToken::Table(..) => None,
        }
    }
}

impl Token {
    pub fn tokenType(&self) -> String {
        match self {
            Token::Identifier(_) => "Identifier".to_string(),
            Token::Whitespace() => "Whitespace".to_string(),
            Token::Keyword(_) => "Keyword".to_string(),
            Token::Punctuation(_) => "Punctuation".to_string(),
            Token::Brackets(_) => "Brackets".to_string(),
            Token::Symbol(_) => "Symbol".to_string(),
            Token::ConfigKey(_) => "ConfigKey".to_string(),
            Token::Function(_) => "Function".to_string(),
            Token::TableType(_) => "TableType".to_string(),
            Token::Decorator(_) => "Decorator".to_string(),
            Token::EOF => "EOF".to_string(),
        }
    }

    pub fn tokenValue(&self) -> String {
        match self {
            Token::Whitespace() => " ".to_string(),
            Token::Identifier(data) => data.to_string(),
            Token::Keyword(data) => data.to_string(),
            Token::Punctuation(data) => data.to_string(),
            Token::Brackets(data) => data.to_string(),
            Token::Symbol(data) => data.to_string(),
            Token::ConfigKey(data) => data.to_string(),
            Token::Function(data) => data.to_string(),
            Token::TableType(data) => data.to_string(),
            Token::Decorator(data) => data.to_string(),
            Token::EOF => "EOF".to_string(),
        }
    }
}
