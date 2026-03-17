use crate::ModelWhere;
use crate::Value;
use crate::WhereQuery;

#[derive(Clone)]
pub struct FindQuery {
    skip: Option<i64>,
    take: Option<i64>,

    query: WhereQuery,
}

impl FindQuery {
    pub fn fields(&self) -> Vec<String> {
        self.query.fields.clone()
    }

    pub fn new(table: String) -> Self {
        Self {
            skip: None,
            take: None,
            query: WhereQuery::new(table),
        }
    }

    pub fn field<T: Value>(&mut self, value: ModelWhere<T>) -> &mut Self {
        self.query.field(value);

        self
    }

    pub fn take(&mut self, value: i64) -> &mut Self {
        if value > 0 {
            self.take = Some(value);
        }

        self
    }

    pub fn skip(&mut self, value: i64) -> &mut Self {
        if value > 0 {
            self.skip = Some(value);
        }

        self
    }

    pub fn params(&self) -> Vec<String> {
        self.query.params()
    }

    pub fn sql(&self, keys: Vec<String>) -> String {
        let mut sql = format!(
            "SELECT {} FROM \"{}\" {}",
            keys.join(", ").trim().to_string(),
            self.query.table,
            self.query.sql()
        );

        if let Some(take) = self.take {
            sql.push_str(&format!(" LIMIT {}", take));
        }

        if let Some(skip) = self.skip {
            if self.take.is_none() {
                sql.push_str(&format!(" LIMIT {}", -1));
            }

            sql.push_str(&format!(" OFFSET {}", skip));
        }

        sql.trim().to_string()
    }
}
