use tokio_postgres::{Row, types::Type};

use crate::{DinocoGenericRow, DinocoResult, DinocoValue};

impl DinocoGenericRow for Row {
    fn get_value(&self, idx: usize) -> DinocoResult<DinocoValue> {
        let col = self.columns()[idx].type_();

        match *col {
            Type::INT2 | Type::INT4 | Type::INT8 => Ok(DinocoValue::Integer(self.try_get(idx)?)),
            Type::TEXT | Type::VARCHAR => Ok(DinocoValue::String(self.try_get(idx)?)),
            Type::BOOL => Ok(DinocoValue::Boolean(self.try_get(idx)?)),
            Type::FLOAT4 | Type::FLOAT8 => Ok(DinocoValue::Float(self.try_get(idx)?)),
            Type::BYTEA => Ok(DinocoValue::Bytes(self.try_get(idx)?)),

            _ => Ok(DinocoValue::Null),
        }
    }
}
