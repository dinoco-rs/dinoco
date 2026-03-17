use std::any::Any;
use std::fmt::Debug;

pub use etanol_databases::{Database, InsertQuery, Value};

#[derive(Debug)]
pub struct Insert {
    pub table: String,
    pub values: Vec<(String, String)>,
}

impl Insert {
    pub fn new(table: String, values: Vec<(String, String)>) -> Self {
        Self { table, values }
    }

    pub fn execute(&self) {
        let mut params = vec![];
        let mut query = InsertQuery::new();

        query.insert(self.table.clone());

        for (key, value) in &self.values {
            if value == &"None" {
                continue;
            }

            query.field(key.clone());

            params.push(value.clone());
        }

        // adiciona error handler
        Database::execute(query.sql(), &params).unwrap();
    }

    pub fn formatType<V: Any + Debug + Clone + Value>(value: V, default: Option<String>) -> String {
        value.toValue(default)
    }
}
