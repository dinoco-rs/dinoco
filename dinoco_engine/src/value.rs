use chrono::{DateTime, NaiveDate, Utc};
use std::convert::TryFrom;

use crate::DinocoError;

#[derive(Debug, Clone, PartialEq)]
pub enum DinocoValue {
    Null,
    Integer(i64),
    Float(f64),
    String(String),
    Boolean(bool),

    Bytes(Vec<u8>),

    Json(serde_json::Value),
    DateTime(DateTime<Utc>),
    Date(NaiveDate),
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

impl From<Vec<u8>> for DinocoValue {
    fn from(value: Vec<u8>) -> Self {
        DinocoValue::Bytes(value)
    }
}

impl From<DateTime<Utc>> for DinocoValue {
    fn from(value: DateTime<Utc>) -> Self {
        DinocoValue::DateTime(value)
    }
}

impl From<NaiveDate> for DinocoValue {
    fn from(value: NaiveDate) -> Self {
        DinocoValue::Date(value)
    }
}

impl TryFrom<DinocoValue> for String {
    type Error = DinocoError;

    fn try_from(value: DinocoValue) -> Result<Self, Self::Error> {
        match value {
            DinocoValue::String(s) => Ok(s),
            DinocoValue::Bytes(b) => String::from_utf8(b).map_err(|_| DinocoError::ParseError("Invalid UTF-8".into())),
            _ => Err(DinocoError::ParseError("Expected String".into())),
        }
    }
}

impl TryFrom<DinocoValue> for i64 {
    type Error = DinocoError;

    fn try_from(value: DinocoValue) -> Result<Self, Self::Error> {
        match value {
            DinocoValue::Integer(i) => Ok(i),
            DinocoValue::Boolean(b) => Ok(if b { 1 } else { 0 }),
            _ => Err(DinocoError::ParseError("Expected i64".into())),
        }
    }
}

impl TryFrom<DinocoValue> for f64 {
    type Error = DinocoError;

    fn try_from(value: DinocoValue) -> Result<Self, Self::Error> {
        match value {
            DinocoValue::Float(f) => Ok(f),
            DinocoValue::Integer(i) => Ok(i as f64),
            _ => Err(DinocoError::ParseError("Expected f64".into())),
        }
    }
}

impl TryFrom<DinocoValue> for bool {
    type Error = DinocoError;

    fn try_from(value: DinocoValue) -> Result<Self, Self::Error> {
        match value {
            DinocoValue::Boolean(b) => Ok(b),
            DinocoValue::Integer(i) => Ok(i != 0),
            _ => Err(DinocoError::ParseError("Expected bool".into())),
        }
    }
}

impl TryFrom<DinocoValue> for Vec<u8> {
    type Error = DinocoError;

    fn try_from(value: DinocoValue) -> Result<Self, Self::Error> {
        match value {
            DinocoValue::Bytes(b) => Ok(b),
            DinocoValue::String(s) => Ok(s.into_bytes()),
            _ => Err(DinocoError::ParseError("Expected bytes".into())),
        }
    }
}

impl TryFrom<DinocoValue> for DateTime<Utc> {
    type Error = DinocoError;

    fn try_from(value: DinocoValue) -> Result<Self, Self::Error> {
        match value {
            DinocoValue::DateTime(dt) => Ok(dt),
            _ => Err(DinocoError::ParseError("Expected DateTime<Utc>".into())),
        }
    }
}

impl TryFrom<DinocoValue> for NaiveDate {
    type Error = DinocoError;

    fn try_from(value: DinocoValue) -> Result<Self, Self::Error> {
        match value {
            DinocoValue::Date(date) => Ok(date),
            _ => Err(DinocoError::ParseError("Expected NaiveDate".into())),
        }
    }
}
