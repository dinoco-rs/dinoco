#[derive(Debug, Clone, PartialEq)]
pub enum DinocoValue {
    Null,
    Integer(i64),
    Float(f64),
    String(String),
    Enum(String, String),
    Boolean(bool),

    Bytes(Vec<u8>),
    // Json(serde_json::Value),
    // DateTime(DateTime<Utc>),
    // Date(NaiveDate),
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

impl From<&String> for DinocoValue {
    fn from(value: &String) -> Self {
        DinocoValue::String(value.clone())
    }
}

impl From<i64> for DinocoValue {
    fn from(value: i64) -> Self {
        DinocoValue::Integer(value)
    }
}

impl From<&i64> for DinocoValue {
    fn from(value: &i64) -> Self {
        DinocoValue::Integer(*value)
    }
}

impl From<f64> for DinocoValue {
    fn from(value: f64) -> Self {
        DinocoValue::Float(value)
    }
}

impl From<&f64> for DinocoValue {
    fn from(value: &f64) -> Self {
        DinocoValue::Float(*value)
    }
}

impl From<bool> for DinocoValue {
    fn from(value: bool) -> Self {
        DinocoValue::Boolean(value)
    }
}

impl From<&bool> for DinocoValue {
    fn from(value: &bool) -> Self {
        DinocoValue::Boolean(*value)
    }
}

impl From<Vec<u8>> for DinocoValue {
    fn from(value: Vec<u8>) -> Self {
        DinocoValue::Bytes(value)
    }
}

impl From<&Vec<u8>> for DinocoValue {
    fn from(value: &Vec<u8>) -> Self {
        DinocoValue::Bytes(value.clone())
    }
}

// impl From<DateTime<Utc>> for DinocoValue {
//     fn from(value: DateTime<Utc>) -> Self {
//         DinocoValue::DateTime(value)
//     }
// }

// impl From<NaiveDate> for DinocoValue {
//     fn from(value: NaiveDate) -> Self {
//         DinocoValue::Date(value)
//     }
// }
