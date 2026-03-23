use std::collections::HashMap;

use pest::Span;

#[derive(Debug, Clone, PartialEq)]
pub struct dinocoError {
    pub message: String,

    pub start_line: usize,
    pub start_column: usize,

    pub end_line: usize,
    pub end_column: usize,
}

impl Default for dinocoError {
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

pub type dinocoResult<T> = Result<T, Vec<dinocoError>>;

#[derive(Debug, Clone, PartialEq)]
pub struct Schema<'a> {
    pub tables: Vec<Table<'a>>,
    pub enums: Vec<Enum<'a>>,
    pub configs: Vec<Config<'a>>,
    pub span: pest::Span<'a>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Config<'a> {
    pub fields: Vec<ConfigField<'a>>,
    pub span: pest::Span<'a>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConfigField<'a> {
    pub name: String,
    pub value: Option<ConfigValue<'a>>,
    pub span: pest::Span<'a>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ConfigValue<'a> {
    String(String),
    Function { name: String, args: Vec<ConfigValue<'a>> },
    Array(Vec<ConfigValue<'a>>),
    Object(Vec<ConfigField<'a>>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Table<'a> {
    pub name: String,
    pub fields: Vec<Field<'a>>,
    pub span: Span<'a>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Enum<'a> {
    pub name: String,
    pub values: Vec<String>,
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
    Custom(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum FunctionCall {
    Uuid,
    Snowflake,
    AutoIncrement,
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

    pub fn from_string(data: &str) -> dinocoResult<Self> {
        if let Some((name, params_with_paren)) = data.split_once('(') {
            if let Some(params) = params_with_paren.strip_suffix(')') {
                match name {
                    "env" => Ok(Self::Env(params.to_string())),
                    "uuid" => Ok(Self::Uuid),
                    "snowflake" => Ok(Self::Snowflake),
                    "autoincrement" => Ok(Self::AutoIncrement),
                    _ => Err(vec![dinocoError::default()]),
                }
            } else {
                Err(vec![dinocoError::default()])
            }
        } else {
            Err(vec![dinocoError::default()])
        }
    }
}
