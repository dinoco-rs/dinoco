use tokio_postgres::{Row, types::Type};

use crate::{DinocoError, DinocoGenericRow, DinocoResult, DinocoValue};

impl DinocoGenericRow for Row {
    fn get_value(&self, idx: usize) -> DinocoResult<DinocoValue> {
        let col = self.columns()[idx].type_();

        match *col {
            Type::INT2 => Ok(self
                .try_get::<_, Option<i16>>(idx)?
                .map(|value| DinocoValue::Integer(value as i64))
                .unwrap_or(DinocoValue::Null)),
            Type::INT4 => Ok(self
                .try_get::<_, Option<i32>>(idx)?
                .map(|value| DinocoValue::Integer(value as i64))
                .unwrap_or(DinocoValue::Null)),
            Type::INT8 => Ok(self
                .try_get::<_, Option<i64>>(idx)?
                .map(DinocoValue::Integer)
                .unwrap_or(DinocoValue::Null)),

            Type::TEXT | Type::VARCHAR | Type::NAME | Type::BPCHAR => Ok(self
                .try_get::<_, Option<String>>(idx)?
                .map(DinocoValue::String)
                .unwrap_or(DinocoValue::Null)),

            Type::BOOL => Ok(self
                .try_get::<_, Option<bool>>(idx)?
                .map(DinocoValue::Boolean)
                .unwrap_or(DinocoValue::Null)),

            Type::FLOAT4 => Ok(self
                .try_get::<_, Option<f32>>(idx)?
                .map(|value| DinocoValue::Float(value as f64))
                .unwrap_or(DinocoValue::Null)),
            Type::FLOAT8 => Ok(self
                .try_get::<_, Option<f64>>(idx)?
                .map(DinocoValue::Float)
                .unwrap_or(DinocoValue::Null)),

            Type::BYTEA => Ok(self
                .try_get::<_, Option<Vec<u8>>>(idx)?
                .map(DinocoValue::Bytes)
                .unwrap_or(DinocoValue::Null)),

            _ => Err(DinocoError::ParseError(format!(
                "Unsupported postgres type {:?} at column {}",
                col, idx
            ))),
        }
    }
}
