use dinoco_derives::Seriable;
use dinoco_engine::{DinocoDatabaseRow, DinocoResult, DinocoRow};

mod database;
mod encoders;

pub use database::*;
pub use encoders::*;

#[derive(Seriable, Debug)]
pub struct SqliteColumnRaw {
    pub name: String,
    pub r#type: String,
    pub notnull: i64,
    pub dflt_value: Option<String>,
}

#[derive(Debug)]
pub struct DatabaseParsedTable {
    pub name: String,
    pub columns: Vec<DatabaseColumn>,
}

#[derive(Seriable, Debug)]
pub struct DataCheck {
    pub has_data: i64,
}

#[derive(Seriable, Debug)]
pub struct DatabaseTable {
    pub name: String,
}

#[derive(Seriable, Debug)]
struct DatabaseEnum {
    pub name: String,
}

#[derive(Seriable, Debug)]
pub struct DatabaseForeignKey {
    pub table_name: String,
    pub constraint_name: String,
}

#[derive(Seriable, Debug)]
pub struct DatabaseColumn {
    pub name: String,
    pub db_type: String,
    pub nullable: bool,
    pub default_value: Option<String>,
}

#[derive(Seriable, Debug)]
pub struct DinocoMigration {
    pub id: i64,
    pub name: String,
    pub schema: Vec<u8>,
}

pub fn to_snake_case(s: &str) -> String {
    let mut snake = String::new();

    for (i, ch) in s.char_indices() {
        if ch.is_uppercase() {
            if i > 0 && !snake.ends_with('_') {
                snake.push('_');
            }

            snake.extend(ch.to_lowercase());
        } else if ch == ' ' || ch == '-' {
            if !snake.ends_with('_') {
                snake.push('_');
            }
        } else {
            snake.push(ch);
        }
    }

    snake.trim_matches('_').to_string()
}
