use std::collections::HashMap;

use pest::Span;

#[derive(Debug, Clone, PartialEq)]
pub struct DinocoError {
    pub message: String,

    pub start_line: usize,
    pub start_column: usize,

    pub end_line: usize,
    pub end_column: usize,
}

impl Default for DinocoError {
    fn default() -> Self {
        Self {
            message: "".to_string(),

            start_line: 0,
            start_column: 0,
            end_line: 0,
            end_column: 0,
        }
    }
}

pub type DinocoResult<T> = Result<T, Vec<DinocoError>>;

#[derive(Debug, Clone, PartialEq)]
pub struct Schema<'a> {
    pub tables: Vec<Table<'a>>,
    pub enums: Vec<Enum<'a>>,
    pub configs: Vec<Config<'a>>,
    pub span: Span<'a>,

    pub total_blocks: usize,
    pub comments: Vec<(usize, String)>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Config<'a> {
    pub position: usize,
    pub total_fields: usize,

    pub comments: Vec<(usize, Span<'a>)>,

    pub fields: Vec<ConfigField<'a>>,
    pub span: Span<'a>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConfigField<'a> {
    pub name: String,
    pub position: usize,

    pub comments: Vec<Span<'a>>,

    pub value: Option<ConfigValue<'a>>,
    pub span: Span<'a>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ConfigValue<'a> {
    String(String, Span<'a>),
    Function { name: String, args: Vec<ConfigValue<'a>>, span: Span<'a> },
    Array(Vec<ConfigValue<'a>>, Span<'a>),
    Object(Vec<ConfigField<'a>>, Span<'a>),
    Comment(Span<'a>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Table<'a> {
    pub name: String,
    pub fields: Vec<Field<'a>>,
    pub span: Span<'a>,

    pub position: usize,
    pub total_fields: usize,

    pub comments: Vec<(usize, Span<'a>)>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Enum<'a> {
    pub position: usize,
    pub total_blocks: usize,
    pub comments: Vec<(usize, Span<'a>)>,

    pub name: String,
    pub values: Vec<(usize, Span<'a>)>,

    pub span: Span<'a>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Relation<'a> {
    pub named_params: HashMap<String, Vec<String>>,
    pub span: Span<'a>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Field<'a> {
    pub name: String,
    pub field_type: FieldType,
    pub default_value: FieldDefaultValue,

    pub is_primary_key: bool,
    pub is_optional: bool,
    pub is_unique: bool,
    pub is_list: bool,

    pub relation: Option<Relation<'a>>,

    pub newlines: usize,
    pub position: usize,
    pub comments: Vec<String>,

    pub span: Span<'a>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum FieldDefaultValue {
    NotDefined,
    String(String),
    Boolean(bool),
    Integer(i64),
    Float(f64),
    Custom(String),
    Function(FunctionCall),
}

#[derive(Debug, Clone, PartialEq)]
pub enum FieldType {
    String,
    Boolean,
    Integer,
    Float,
    Json,
    DateTime,
    Custom(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum FunctionCall {
    Uuid,
    Snowflake,
    AutoIncrement,
    Now,
    Env(String),
}

impl FunctionCall {
    pub fn is_func(data: &str) -> bool {
        if let Some((_name, params_and_rest)) = data.split_once('(') {
            params_and_rest.ends_with(')')
        } else {
            false
        }
    }

    pub fn from_string(data: &str) -> DinocoResult<Self> {
        if let Some((name, params_with_paren)) = data.split_once('(') {
            if let Some(params) = params_with_paren.strip_suffix(')') {
                match name {
                    "env" => Ok(Self::Env(params.to_string())),
                    "uuid" => Ok(Self::Uuid),
                    "snowflake" => Ok(Self::Snowflake),
                    "now" => Ok(Self::Now),
                    "autoincrement" => Ok(Self::AutoIncrement),
                    _ => Err(vec![DinocoError::default()]),
                }
            } else {
                Err(vec![DinocoError::default()])
            }
        } else {
            Err(vec![DinocoError::default()])
        }
    }
}

impl<'a> ConfigValue<'a> {
    pub fn span(&self) -> Span<'a> {
        match self {
            ConfigValue::String(_, s) => s.clone(),
            ConfigValue::Function { span, .. } => span.clone(),
            ConfigValue::Array(_, s) => s.clone(),
            ConfigValue::Object(_, s) => s.clone(),
            ConfigValue::Comment(s) => s.clone(),
        }
    }
}
