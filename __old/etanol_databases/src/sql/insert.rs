pub struct InsertQuery {
    sql: String,
    values: Vec<String>,
    fields: Vec<String>,
}

impl InsertQuery {
    pub fn new() -> Self {
        Self {
            sql: String::new(),
            values: vec![],
            fields: vec![],
        }
    }

    pub fn insert(&mut self, table: String) -> &mut Self {
        self.sql.push_str(&format!("INSERT INTO {}", table));

        self
    }

    pub fn field(&mut self, name: String) -> &mut Self {
        self.values.push(format!("?{}", self.values.len() + 1));
        self.fields.push(format!("{}", name));

        self
    }

    pub fn sql(&self) -> String {
        let values = self.values.join(", ").trim().to_string();
        let fields = self.fields.join(", ").trim().to_string();

        format!("{} ({}) VALUES ({});", self.sql, fields, values)
    }
}
