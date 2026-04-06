use std::str;

use chrono::{DateTime, NaiveDate, Utc};
use rusqlite::Row as RusqliteRow;
use rusqlite::types::ValueRef;

use crate::{DinocoGenericRow, DinocoResult, DinocoValue};

impl<'stmt> DinocoGenericRow for RusqliteRow<'stmt> {
    fn get_value(&self, idx: usize) -> DinocoResult<DinocoValue> {
        let value_ref = self.get_ref(idx).unwrap();

        match value_ref {
            ValueRef::Null => Ok(DinocoValue::Null),
            ValueRef::Integer(i) => Ok(DinocoValue::Integer(i)),
            ValueRef::Real(f) => Ok(DinocoValue::Float(f)),
            ValueRef::Text(t) => {
                let text_str = str::from_utf8(t).unwrap_or_default().to_string();

                // if let Ok(date) = NaiveDate::parse_from_str(&text_str, "%Y-%m-%d") {
                //     return Ok(DinocoValue::Date(date));
                // }

                // if let Ok(datetime) = DateTime::parse_from_rfc3339(&text_str) {
                //     return Ok(DinocoValue::DateTime(datetime.with_timezone(&Utc)));
                // }

                Ok(DinocoValue::String(text_str))
            }
            ValueRef::Blob(b) => Ok(DinocoValue::Bytes(b.to_vec())),
        }
    }
}
