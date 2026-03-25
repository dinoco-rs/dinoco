use super::{DinocoDatabase, DinocoDatabaseUrl};

pub struct DinocoSchema {
    pub config: DinocoConfig,
}

pub struct DinocoConfig {
    pub database: DinocoDatabase,
    pub database_url: DinocoDatabaseUrl,
    pub read_replicas: Vec<DinocoDatabaseUrl>,
}

impl DinocoSchema {
    pub fn new(config: DinocoConfig) -> Self {
        Self { config }
    }
}

impl DinocoConfig {
    pub fn new(database: DinocoDatabase, database_url: DinocoDatabaseUrl, read_replicas: Vec<DinocoDatabaseUrl>) -> Self {
        Self {
            database,
            database_url,
            read_replicas,
        }
    }
}

impl ToString for DinocoSchema {
    fn to_string(&self) -> String {
        let mut schema = String::new();

        schema.push_str(&self.config.to_string());

        schema
    }
}

impl ToString for DinocoConfig {
    fn to_string(&self) -> String {
        let mut config = String::from("config {");

        config.push_str(&"database = ");
        config.push_str(&self.database.to_string());

        config.push_str(&"\n");

        config.push_str(&" database_url = ");
        config.push_str(&self.database_url.to_string());

        if !self.read_replicas.is_empty() {
            config.push_str(&"\n");

            config.push_str(&" read_replicas = [");

            config.push_str(&self.read_replicas.iter().map(|x| x.to_string()).collect::<Vec<String>>().join(","));

            config.push_str(&"]");
        }

        config.push_str("}");

        config
    }
}
