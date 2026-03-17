use super::Table;
use crate::Database;

pub struct Migration {
    pub tables: Vec<Table>,
}

impl Migration {
    pub fn new() -> Migration {
        Migration { tables: vec![] }
    }

    pub fn createTable(&mut self, name: String) -> &mut Table {
        let index = self.tables.len();

        self.tables.push(Table {
            name,
            columns: vec![],
        });

        &mut self.tables[index]
    }

    pub fn make(&mut self) -> Vec<String> {
        let mut tables = vec![];

        for table in self.tables.clone() {
            tables.push(Database::createTable(table.name, table.columns));
        }

        tables
    }
}
