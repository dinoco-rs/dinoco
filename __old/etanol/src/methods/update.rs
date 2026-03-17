use crate::{ModelWhere, Value};
use etanol_databases::Database;

pub use etanol_databases::{QueryValue, UpdateError};

use etanol_databases::UpdateQuery;
use std::fmt::Debug;

use crate::FindTrait;

pub struct Update<M: FindTrait + Debug> {
    _model: M,
    query: UpdateQuery,
}

impl<M: FindTrait + Debug> Update<M> {
    pub fn new(table: String, _model: M) -> Self {
        Self {
            _model,
            query: UpdateQuery::new(table),
        }
    }

    pub fn field<V: Value>(&mut self, value: ModelWhere<V>) -> &mut Self {
        self.query.field(value);

        self
    }
    pub fn value<V: Value>(&mut self, value: QueryValue<V>) -> &mut Self {
        self.query.value(value);

        self
    }

    pub fn execute(&self) -> Result<(), UpdateError> {
        match self.query.sql() {
            Ok(sql) => {
                Database::execute(sql, &self.query.params()).unwrap();

                Ok(())
            }
            Err(e) => Err(e),
        }
    }
}
