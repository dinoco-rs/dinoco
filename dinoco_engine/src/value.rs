use crate::{DinocoError, DinocoResult, FromDinocoValue, RowExt};
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, PartialEq)]
pub enum DinocoValue {
    Null,
    Integer(i64),
    Float(f64),
    String(String),
    Boolean(bool),

    Json(serde_json::Value),
    DateTime(DateTime<Utc>),
}

impl DinocoValue {
    pub fn as_value<T: FromDinocoValue>(&self) -> Result<T, DinocoError> {
        T::from_value(self)
    }
}

impl RowExt for Vec<DinocoValue> {
    fn get_value<T: FromDinocoValue>(&self, index: usize) -> DinocoResult<T> {
        let value = self.get(index).ok_or(DinocoError::ColumnNotFound)?;

        value.as_value()
    }
}

impl FromDinocoValue for String {
    fn from_value(value: &DinocoValue) -> DinocoResult<Self> {
        match value {
            DinocoValue::String(v) => Ok(v.clone()),
            _ => Err(DinocoError::TypeMismatch),
        }
    }
}

impl FromDinocoValue for i64 {
    fn from_value(value: &DinocoValue) -> DinocoResult<Self> {
        match value {
            DinocoValue::Integer(v) => Ok(*v),
            _ => Err(DinocoError::TypeMismatch),
        }
    }
}

impl FromDinocoValue for bool {
    fn from_value(value: &DinocoValue) -> DinocoResult<Self> {
        match value {
            DinocoValue::Boolean(v) => Ok(*v),
            _ => Err(DinocoError::TypeMismatch),
        }
    }
}

impl FromDinocoValue for f64 {
    fn from_value(value: &DinocoValue) -> DinocoResult<Self> {
        match value {
            DinocoValue::Float(v) => Ok(*v),
            _ => Err(DinocoError::TypeMismatch),
        }
    }
}

impl From<&str> for DinocoValue {
    fn from(value: &str) -> Self {
        DinocoValue::String(value.to_string())
    }
}

impl From<String> for DinocoValue {
    fn from(value: String) -> Self {
        DinocoValue::String(value.to_string())
    }
}

impl From<i64> for DinocoValue {
    fn from(value: i64) -> Self {
        DinocoValue::Integer(value)
    }
}

impl From<f64> for DinocoValue {
    fn from(value: f64) -> Self {
        DinocoValue::Float(value)
    }
}

impl From<bool> for DinocoValue {
    fn from(value: bool) -> Self {
        DinocoValue::Boolean(value)
    }
}
