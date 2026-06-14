pub enum DinocoDatabase {
    Mysql,
    Sqlite,
    Postgres,
}

pub enum DinocoDatabaseUrl {
    Env(String),
    String(String),
}

impl DinocoDatabase {
    pub fn from_str(value: &str) -> Option<Self> {
        match value {
            "mysql" => Some(Self::Mysql),
            "postgresql" => Some(Self::Postgres),
            "sqlite" => Some(Self::Sqlite),
            _ => None,
        }
    }

    pub fn to_string(&self) -> String {
        match self {
            Self::Mysql => "\"mysql\"".to_string(),
            Self::Sqlite => "\"sqlite\"".to_string(),
            Self::Postgres => "\"postgresql\"".to_string(),
        }
    }
}

impl ToString for DinocoDatabaseUrl {
    fn to_string(&self) -> String {
        match self {
            Self::Env(v) => format!("env(\"{}\")", v),
            Self::String(v) => format!("\"{}\"", v),
        }
    }
}
