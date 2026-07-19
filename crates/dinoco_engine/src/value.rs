use chrono::{DateTime, NaiveDate, Utc};

#[derive(Debug, Clone, PartialEq)]
pub enum DinocoValue {
    Null,
    Integer(i64),
    Float(f64),
    String(String),
    Enum(String, String),
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

macro_rules! impl_integer_value {
    ($($ty:ty),* $(,)?) => {
        $(
            impl From<$ty> for DinocoValue {
                fn from(value: $ty) -> Self {
                    DinocoValue::Integer(value as i64)
                }
            }

            impl From<&$ty> for DinocoValue {
                fn from(value: &$ty) -> Self {
                    DinocoValue::Integer(*value as i64)
                }
            }
        )*
    };
}

impl_integer_value!(i8, i16, i32, i128, isize, u8, u16, u32, u64, u128, usize);

impl From<f64> for DinocoValue {
    fn from(value: f64) -> Self {
        DinocoValue::Float(value)
    }
}

impl From<f32> for DinocoValue {
    fn from(value: f32) -> Self {
        DinocoValue::Float(value as f64)
    }
}

impl From<&f32> for DinocoValue {
    fn from(value: &f32) -> Self {
        DinocoValue::Float(*value as f64)
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

impl From<serde_json::Value> for DinocoValue {
    fn from(value: serde_json::Value) -> Self {
        DinocoValue::Json(value)
    }
}

impl From<&serde_json::Value> for DinocoValue {
    fn from(value: &serde_json::Value) -> Self {
        DinocoValue::Json(value.clone())
    }
}

impl From<DateTime<Utc>> for DinocoValue {
    fn from(value: DateTime<Utc>) -> Self {
        DinocoValue::DateTime(value)
    }
}

impl From<&DateTime<Utc>> for DinocoValue {
    fn from(value: &DateTime<Utc>) -> Self {
        DinocoValue::DateTime(*value)
    }
}

impl From<NaiveDate> for DinocoValue {
    fn from(value: NaiveDate) -> Self {
        DinocoValue::Date(value)
    }
}

impl From<&NaiveDate> for DinocoValue {
    fn from(value: &NaiveDate) -> Self {
        DinocoValue::Date(*value)
    }
}
