use chrono::{DateTime, NaiveDate, NaiveDateTime, NaiveTime, Utc};
use mysql_async::{Row, Value as MyValue};

use crate::{DinocoError, DinocoGenericRow, DinocoResult, DinocoValue};

impl DinocoGenericRow for Row {
    fn get_value(&self, idx: usize) -> DinocoResult<DinocoValue> {
        let val = self.as_ref(idx).ok_or_else(|| DinocoError::ParseError(format!("Column {} not found", idx)))?;

        match val {
            MyValue::NULL => Ok(DinocoValue::Null),

            MyValue::Int(i) => Ok(DinocoValue::Integer(*i)),
            MyValue::UInt(u) => Ok(DinocoValue::Integer(*u as i64)),

            MyValue::Float(f) => Ok(DinocoValue::Float(*f as f64)),
            MyValue::Double(f) => Ok(DinocoValue::Float(*f)),

            MyValue::Bytes(bytes) => match String::from_utf8(bytes.clone()) {
                Ok(s) => Ok(DinocoValue::String(s)),
                Err(_) => Ok(DinocoValue::Bytes(bytes.clone())),
            },

            MyValue::Date(year, month, day, hour, min, sec, micros) => {
                let date = NaiveDate::from_ymd_opt(*year as i32, *month as u32, *day as u32).ok_or_else(|| DinocoError::ParseError("Invalid date".into()))?;

                if *hour == 0 && *min == 0 && *sec == 0 && *micros == 0 {
                    return Ok(DinocoValue::Date(date));
                }

                let time = NaiveTime::from_hms_micro_opt(*hour as u32, *min as u32, *sec as u32, *micros).ok_or_else(|| DinocoError::ParseError("Invalid time".into()))?;
                let naive = NaiveDateTime::new(date, time);
                let utc: DateTime<Utc> = DateTime::<Utc>::from_naive_utc_and_offset(naive, Utc);

                Ok(DinocoValue::DateTime(utc))
            }

            MyValue::Time(_, _, _, _, _, _) => Err(DinocoError::ParseError("MySQL TIME cannot be mapped to DateTime<Utc>".into())),
        }
    }
}
