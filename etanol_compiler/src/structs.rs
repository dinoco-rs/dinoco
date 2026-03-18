#[derive(Debug, Clone, PartialEq)]
pub struct Schema {
    pub tables: Vec<Table>,
    pub enums: Vec<Enum>,
}

pub type EtanolResult<T> = Result<T, String>;

#[derive(Debug, Clone, PartialEq)]
pub struct Table {
    pub name: String,
    pub fields: Vec<Field>,
    pub line: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub enum FieldDefaultValue {
    String(String),
    Boolean(bool),
    Integer(i64),
    Float(f64),
    Function(FunctionCall),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Enum {
    pub name: String,
    pub values: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    pub name: String,
    pub field_type: FieldType,
    pub default_value: Option<FieldDefaultValue>,

    pub line: usize,

    pub primary_key: bool,
    pub optional: bool,
    pub unique: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum FieldType {
    String,
    Boolean,
    Integer,
    Float,
}

impl FieldType {
    pub fn from_string(data: &str) -> EtanolResult<Self> {
        match data {
            "Boolean" => Ok(Self::Boolean),
            "String" => Ok(Self::String),
            "Integer" => Ok(Self::Integer),
            "Float" => Ok(Self::Float),

            _ => Err(data.to_string()),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum FunctionCall {
    Uuid,
    Snowflake,
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

    pub fn from_string(data: &str) -> EtanolResult<Self> {
        if let Some((name, params_with_paren)) = data.split_once('(') {
            if let Some(params) = params_with_paren.strip_suffix(')') {
                match name {
                    "env" => Ok(Self::Env(params.to_string())),
                    "uuid" => Ok(Self::Uuid),
                    "snowflake" => Ok(Self::Snowflake),
                    _ => Err(data.to_string()),
                }
            } else {
                Err(data.to_string())
            }
        } else {
            Err(data.to_string())
        }
    }
}
