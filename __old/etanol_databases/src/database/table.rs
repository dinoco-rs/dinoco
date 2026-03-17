use super::Column;

#[derive(Debug, Clone)]
pub struct Table {
    pub name: String,
    pub columns: Vec<Column>,
}

impl Table {
    pub fn addColumn(&mut self, name: String, columnType: String) -> &mut Column {
        let index = self.columns.len();

        self.columns.push(Column {
            name,
            columnType,
            isOptional: false,
            isPrimary: false,
            uuid: false,
            autoincrement: false,
            default: None,
        });

        &mut self.columns[index]
    }
}
