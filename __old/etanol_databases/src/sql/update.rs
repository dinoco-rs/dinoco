use crate::ModelWhere;
use crate::Value;
use crate::WhereQuery;

#[derive(Debug)]
pub struct QueryValue<T: Value>(pub &'static str, pub T);

#[derive(Debug)]
pub enum UpdateError {
    NoWhereCondition,
}

#[derive(Debug)]
pub struct UpdateQuery {
    query: WhereQuery,

    values: Vec<String>,
    keys: Vec<String>,
}

impl UpdateQuery {
    pub fn new(table: String) -> Self {
        Self {
            query: WhereQuery::new(table),

            keys: vec![],
            values: vec![],
        }
    }

    pub fn field<T: Value>(&mut self, value: ModelWhere<T>) -> &mut Self {
        self.query.field(value);

        self
    }

    pub fn value<T: Value>(&mut self, value: QueryValue<T>) -> &mut Self {
        self.values.push(value.1.toValue(None));
        self.keys.push(value.0.to_string());

        self
    }

    pub fn params(&self) -> Vec<String> {
        let mut params = vec![];

        params.append(&mut self.query.params());
        params.append(&mut self.values.clone());

        params
    }

    pub fn sql(&self) -> Result<String, UpdateError> {
        let _where = self.query.sql();
        let params = self.query.params().len() + 1;

        let mut keys = vec![];

        for key in &self.keys {
            keys.push(format!("{} = ?{}", key, params + keys.len()));
        }

        let sql = format!(
            "UPDATE \"{}\" SET {} {}",
            self.query.table,
            keys.join(", "),
            _where
        );

        Ok(sql)
    }
}
