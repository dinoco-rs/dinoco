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
        DinocoValue::String(value)
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

// `isize` is at most 64 bits wide on every target Rust supports, so it
// always fits an `Integer`.
const _: () = assert!(std::mem::size_of::<isize>() <= std::mem::size_of::<i64>());

/// Integers that always fit Dinoco's 64-bit signed `Integer`.
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

impl_integer_value!(i8, i16, i32, isize, u8, u16, u32);

/// A value of a wider integer type that does not fit Dinoco's 64-bit signed
/// `Integer` (`BIGINT`), returned instead of silently truncating it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntegerOutOfRange {
    value: String,
    type_name: &'static str,
}

impl IntegerOutOfRange {
    /// The rejected value, in decimal.
    pub fn value(&self) -> &str {
        &self.value
    }

    /// The Rust type the value came from, such as `u64`.
    pub fn type_name(&self) -> &'static str {
        self.type_name
    }
}

impl std::fmt::Display for IntegerOutOfRange {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "{} value {} does not fit a 64-bit signed integer (the range of Dinoco `Integer` columns)",
            self.type_name, self.value
        )
    }
}

impl std::error::Error for IntegerOutOfRange {}

/// Integers wider than `i64` in at least part of their range. They convert
/// with `DinocoValue::try_from(value)`; `From` is not implemented so an
/// out-of-range value can never be truncated into a different number.
macro_rules! impl_checked_integer_value {
    ($($ty:ident),* $(,)?) => {
        $(
            impl TryFrom<$ty> for DinocoValue {
                type Error = IntegerOutOfRange;

                fn try_from(value: $ty) -> Result<Self, Self::Error> {
                    i64::try_from(value)
                        .map(DinocoValue::Integer)
                        .map_err(|_| IntegerOutOfRange { value: value.to_string(), type_name: stringify!($ty) })
                }
            }

            impl TryFrom<&$ty> for DinocoValue {
                type Error = IntegerOutOfRange;

                fn try_from(value: &$ty) -> Result<Self, Self::Error> {
                    DinocoValue::try_from(*value)
                }
            }
        )*
    };
}

impl_checked_integer_value!(u64, usize, u128, i128);

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

#[cfg(test)]
mod tests {
    use super::{DinocoValue, IntegerOutOfRange};

    #[test]
    fn narrow_integers_convert_losslessly() {
        assert_eq!(DinocoValue::from(u32::MAX), DinocoValue::Integer(i64::from(u32::MAX)));
        assert_eq!(DinocoValue::from(i8::MIN), DinocoValue::Integer(-128));
        assert_eq!(DinocoValue::from(isize::MIN), DinocoValue::Integer(isize::MIN as i64));
    }

    #[test]
    fn wide_integers_convert_only_when_they_fit() {
        assert_eq!(DinocoValue::try_from(42_u64), Ok(DinocoValue::Integer(42)));
        assert_eq!(DinocoValue::try_from(&-7_i128), Ok(DinocoValue::Integer(-7)));
        assert_eq!(DinocoValue::try_from(i64::MAX as u64), Ok(DinocoValue::Integer(i64::MAX)));

        let error: IntegerOutOfRange = DinocoValue::try_from(u64::MAX).unwrap_err();
        assert_eq!(error.value(), "18446744073709551615");
        assert_eq!(error.type_name(), "u64");
        assert!(error.to_string().contains("does not fit a 64-bit signed integer"));
        assert!(DinocoValue::try_from(i128::MIN).is_err());
        assert!(DinocoValue::try_from(u128::from(u64::MAX)).is_err());
        assert!(DinocoValue::try_from(usize::MAX).is_err());
    }
}
