use crate::FunctionCall;

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedSchema {
    pub config: ParsedConfig,
    pub enums: Vec<ParsedEnum>,
    pub tables: Vec<ParsedTable>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedConfig {
    pub database: Database,
    pub database_url: ConnectionUrl,
    pub read_replicas: Vec<ConnectionUrl>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Database {
    Mysql,
    Postgresql,
    Sqlite,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ConnectionUrl {
    Literal(String),
    Env(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedEnum {
    pub name: String,
    pub values: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedTable {
    pub name: String,
    pub fields: Vec<ParsedField>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ReferentialAction {
    Cascade,
    SetNull,
    SetDefault,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ParsedRelation {
    NotDefined,

    /// 1:1 - Lado que guarda a Foreign Key (Owner)
    OneToOneOwner(Option<String>, Vec<String>, Vec<String>, Option<ReferentialAction>, Option<ReferentialAction>),

    /// 1:1 - Lado passivo (Inverse)
    OneToOneInverse(Option<String>),

    /// 1:N - Lado singular que guarda a Foreign Key (Owner)
    ManyToOne(Option<String>, Vec<String>, Vec<String>, Option<ReferentialAction>, Option<ReferentialAction>),

    /// 1:N - Lado lista (Inverse)
    OneToMany(Option<String>),

    /// N:M - Nenhuma guarda a FK (cria a tabela de junção)
    ManyToMany(Option<String>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedField {
    pub name: String,
    pub field_type: ParsedFieldType,

    pub is_primary_key: bool,
    pub is_optional: bool,
    pub is_unique: bool,
    pub is_list: bool,

    pub relation: ParsedRelation,
    pub default_value: ParsedFieldDefault,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ParsedFieldType {
    String,
    Boolean,
    Integer,
    Float,
    Json,
    DateTime,
    Enum(String),
    Relation(String),
}

impl ToString for ParsedFieldType {
    fn to_string(&self) -> String {
        match self {
            ParsedFieldType::String => "String".to_string(),
            ParsedFieldType::Boolean => "Boolean".to_string(),
            ParsedFieldType::Integer => "Integer".to_string(),
            ParsedFieldType::Float => "Float or Integer".to_string(),
            ParsedFieldType::Json => "Json object or Array".to_string(),
            ParsedFieldType::DateTime => "Time in the utc".to_string(),
            ParsedFieldType::Enum(name) => name.clone(),
            ParsedFieldType::Relation(name) => name.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ParsedFieldDefault {
    NotDefined,
    String(String),
    Boolean(bool),
    Integer(i64),
    Float(f64),
    EnumValue(String),
    Function(FunctionCall),
}

impl ConnectionUrl {
    pub fn is_valid(&self) -> bool {
        match self {
            ConnectionUrl::Literal(url) => url.starts_with("postgresql://") || url.starts_with("mysql://") || url.starts_with("file:"),
            ConnectionUrl::Env(_) => true,
        }
    }
}
