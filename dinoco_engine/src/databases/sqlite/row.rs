use std::str;

use chrono::{DateTime, NaiveDate, NaiveDateTime, Utc};
use rusqlite::Row as RusqliteRow;
use rusqlite::types::ValueRef;

use crate::{DinocoError, DinocoGenericRow, DinocoResult, DinocoValue};

impl<'stmt> DinocoGenericRow for RusqliteRow<'stmt> {
    fn get_value(&self, idx: usize) -> DinocoResult<DinocoValue> {
        let value_ref = self.get_ref(idx).map_err(|error| match error {
            rusqlite::Error::InvalidColumnIndex(invalid_index) => {
                DinocoError::ParseError(format!("Column index {} not found in sqlite row", invalid_index))
            }
            other => DinocoError::from(other),
        })?;

        match value_ref {
            ValueRef::Null => Ok(DinocoValue::Null),
            ValueRef::Integer(i) => Ok(DinocoValue::Integer(i)),
            ValueRef::Real(f) => Ok(DinocoValue::Float(f)),
            ValueRef::Text(t) => {
                let text_str = str::from_utf8(t).unwrap_or_default().to_string();

                if let Ok(date) = NaiveDate::parse_from_str(&text_str, "%Y-%m-%d") {
                    return Ok(DinocoValue::Date(date));
                }

                if let Ok(datetime) = DateTime::parse_from_rfc3339(&text_str) {
                    return Ok(DinocoValue::DateTime(datetime.with_timezone(&Utc)));
                }

                if let Some(value) = text_str.strip_suffix(" UTC") {
                    if let Ok(datetime) = NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S%.f") {
                        return Ok(DinocoValue::DateTime(DateTime::<Utc>::from_naive_utc_and_offset(datetime, Utc)));
                    }

                    if let Ok(datetime) = NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S") {
                        return Ok(DinocoValue::DateTime(DateTime::<Utc>::from_naive_utc_and_offset(datetime, Utc)));
                    }
                }

                if let Ok(datetime) = NaiveDateTime::parse_from_str(&text_str, "%Y-%m-%d %H:%M:%S%.f") {
                    return Ok(DinocoValue::DateTime(DateTime::<Utc>::from_naive_utc_and_offset(datetime, Utc)));
                }

                if let Ok(datetime) = NaiveDateTime::parse_from_str(&text_str, "%Y-%m-%d %H:%M:%S") {
                    return Ok(DinocoValue::DateTime(DateTime::<Utc>::from_naive_utc_and_offset(datetime, Utc)));
                }

                Ok(DinocoValue::String(text_str))
            }
            ValueRef::Blob(b) => Ok(DinocoValue::Bytes(b.to_vec())),
        }
    }
}
