#[derive(Debug, Clone)]
pub struct Column {
    pub name: String,
    pub columnType: String,
    pub isOptional: bool,
    pub uuid: bool,
    pub autoincrement: bool,
    pub isPrimary: bool,
    pub default: Option<String>,
}

impl Column {
    pub fn default(&mut self, value: Option<String>) -> &mut Self {
        self.default = value;

        self
    }

    pub fn primaryKey(&mut self, value: bool) -> &mut Self {
        self.isPrimary = value;

        self
    }

    pub fn nullable(&mut self, value: bool) -> &mut Self {
        self.isOptional = value;

        self
    }

    pub fn uuid(&mut self, value: bool) -> &mut Self {
        self.uuid = value;

        self
    }

    pub fn autoincrement(&mut self, value: bool) -> &mut Self {
        self.autoincrement = value;

        self
    }
}
