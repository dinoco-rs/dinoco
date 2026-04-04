mod mysql;
mod postgresql;
mod sqlite;

use dinoco_derives::Rowable;

pub use mysql::*;
pub use postgresql::*;
pub use sqlite::*;

use crate::{DinocoGenericRow, DinocoResult, DinocoRow};

pub struct DatabaseParsedTable {
    pub name: String,
    pub columns: Vec<DatabaseColumn>,
}

pub struct DatabaseParsedEnum {
    pub name: String,
    pub values: Vec<String>,
}

#[derive(Rowable, Debug)]
pub struct DatabaseTable {
    pub name: String,
}

#[derive(Rowable, Debug)]
pub struct DatabaseColumn {
    pub name: String,
    pub db_type: String,
    pub nullable: bool,
    pub default_value: Option<String>,
    pub extra: Option<String>,
}

#[derive(Rowable, Debug)]
pub struct DatabaseEnumRaw {
    pub name: String,
    pub value: String,
}

#[derive(Rowable, Debug)]
pub struct DatabaseForeignKey {
    pub table_name: String,
    pub constraint_name: String,

    pub column_name: String,
    pub foreign_table_name: String,
    pub foreign_column_name: String,
}

#[derive(Rowable, Debug)]
pub struct DatabaseIndex {
    pub table_name: String,
    pub index_name: String,
    pub column_name: String,
    pub is_unique: bool,
}
