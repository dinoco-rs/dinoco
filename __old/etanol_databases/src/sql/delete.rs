use crate::ModelWhere;
use crate::Value;
use crate::WhereQuery;

#[derive(Debug)]
pub enum DeleteError {}

#[derive(Debug)]
pub struct DeleteQuery {
    query: WhereQuery,
}

impl DeleteQuery {
    pub fn new(table: String) -> Self {
        Self {
            query: WhereQuery::new(table),
        }
    }

    pub fn field<T: Value>(&mut self, value: ModelWhere<T>) -> &mut Self {
        self.query.field(value);

        self
    }

    pub fn params(&self) -> Vec<String> {
        self.query.params()
    }

    pub fn sql(&self) -> Result<String, DeleteError> {
        let _where = self.query.sql();

        let sql = format!("DELETE FROM \"{}\" {}", self.query.table, _where);

        Ok(sql)
    }
}
