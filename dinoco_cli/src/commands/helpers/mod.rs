use dinoco_derives::Seriable;
use dinoco_engine::{DinocoDatabaseRow, DinocoResult, DinocoRow};

mod database;
mod encoders;

pub use database::*;
pub use encoders::*;

#[derive(Debug)]
pub struct DatabaseParsedTable {
    pub name: String,
    pub columns: Vec<DatabaseColumn>,
    pub primary_keys: Vec<String>,
    pub foreign_keys: Vec<DatabaseForeignKey>,
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
pub struct DatabaseColumn {
    pub name: String,
    pub db_type: String,
    pub nullable: bool,
    pub default: Option<String>,
}

#[derive(Seriable, Debug)]
pub struct DatabaseForeignKey {
    pub column: String,
    pub references_table: String,
    pub references_column: String,
}

#[derive(Seriable, Debug)]
pub struct DinocoMigration {
    pub id: i64,
    pub name: String,
    pub schema: Vec<u8>,
}
