use crate::{ModelWhere, Value};

use etanol_databases::{Database, DeleteQuery};

pub struct Delete {
    query: DeleteQuery,
}

impl Delete {
    pub fn new(table: String) -> Self {
        Self {
            query: DeleteQuery::new(table),
        }
    }

    pub fn field<V: Value>(&mut self, value: ModelWhere<V>) -> &mut Self {
        self.query.field(value);

        self
    }

    pub fn execute(&self) {
        match self.query.sql() {
            Ok(sql) => {
                Database::execute(sql, &self.query.params()).unwrap();

                // Ok(())
            }
            Err(e) => {}
        }
    }
}
