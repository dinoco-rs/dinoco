use std::fs::File;
use std::path::PathBuf;
use std::slice::Iter;
use std::sync::{Arc, Mutex};

use etanol_utils::EtanolError;
use rusqlite::{types::ValueRef, Connection, Error};

pub use rusqlite::{params_from_iter, Params, ParamsFromIter};

use crate::Column;

lazy_static::lazy_static! {
    static ref SQLITE_CONNECTION: Arc<Mutex<Option<Connection>>> = Arc::new(Mutex::new(None));
}

pub fn createParams(params: &[String]) -> ParamsFromIter<Iter<String>> {
    params_from_iter(params.iter())
}

#[derive(Debug)]
pub struct Database {}

impl Database {
    fn connectionError() {
        EtanolError::new(
            format!(
                "Connection to database is not initialized. Please call `create_connection` first."
            ),
            "DatabaseConnection".to_string(),
        );
    }

    pub fn createTable(name: String, columns: Vec<Column>) -> String {
        let mut sql = String::from("--Create Table\n");

        sql.push_str(&format!("CREATE TABLE IF NOT EXISTS \"{}\" (\n", name));

        for column in columns {
            sql.push_str(&format!(
                "   \"{}\" {}",
                column.name,
                Database::databaseType(column.columnType)
            ));

            if !column.isOptional {
                sql.push_str(" NOT NULL");
            }

            if column.isPrimary {
                sql.push_str(" PRIMARY KEY");
            }

            if column.autoincrement {
                sql.push_str(" AUTOINCREMENT");
            }

            sql.push_str(",\n");
        }

        sql.remove(sql.len() - 2);
        sql.push_str(");\n");

        sql
    }

    pub fn createConnection(url: String) -> Result<(), Error> {
        if !url.ends_with(".sqlite") {
            EtanolError::new(
                format!("Config database_url '{}' not supported", url),
                "DatabaseNotSupported".to_string(),
            );
        }

        let url = format!("etanol/{}", url);
        let path = PathBuf::from(url.clone());
        if !path.exists() {
            match File::create(path) {
                Ok(_) => {}
                Err(e) => {
                    EtanolError::new(
                        format!("Could not create database file: {}", e),
                        "FileSystemError".to_string(),
                    );
                }
            }
        }

        if SQLITE_CONNECTION.lock().unwrap().is_none() {
            if let Ok(conn) = Connection::open(url) {
                *SQLITE_CONNECTION.lock().unwrap() = Some(conn);

                return Ok(());
            }

            EtanolError::new(
                format!("Could not create database connection"),
                "DatabaseConnection".to_string(),
            );
        }

        Ok(())
    }

    pub fn executeWithResults(
        sql: String,
        params: &[String],
    ) -> Result<Vec<Vec<(String, String)>>, Error> {
        if let Some(conn) = &*SQLITE_CONNECTION.lock().unwrap() {
            let mut stmt = conn.prepare(&sql).unwrap();

            let names = stmt
                .column_names()
                .into_iter()
                .map(|s| String::from(s))
                .collect::<Vec<_>>();

            let mut rows = stmt.query(createParams(params)).unwrap();
            let mut models = Vec::new();

            while let Ok(row) = rows.next() {
                if let Some(row) = row {
                    let mut model = vec![];

                    for name in names.iter() {
                        let value = match row.get_ref_unwrap(name.as_ref()) {
                            ValueRef::Integer(i) => i.to_string(),
                            ValueRef::Text(s) => String::from_utf8(s.to_vec()).unwrap(),
                            _ => "None".to_string(),
                        };

                        model.push((name.clone(), value));
                    }

                    models.push(model);
                } else {
                    break;
                }
            }

            return Ok(models);
        }

        Database::connectionError();

        Ok(vec![])
    }

    pub fn execute(sql: String, params: &[String]) -> Result<(), Error> {
        if let Some(conn) = &*SQLITE_CONNECTION.lock().unwrap() {
            conn.execute(&sql, params_from_iter(params.iter())).unwrap();

            return Ok(());
        }

        Database::connectionError();

        Ok(())
    }

    pub fn databaseType(columnType: String) -> String {
        match columnType.as_str() {
            "Integer" => String::from("INTEGER"),
            "String" => String::from("TEXT"),
            "Boolean" => String::from("BOOLEAN"),
            _ => {
                panic!("Invalid column type: {}", columnType);
            }
        }
    }

    pub fn formatQuery(query: String) -> String {
        query
            .split("\n")
            .filter(|x| !x.to_string().starts_with("--"))
            .collect::<Vec<&str>>()
            .join("")
            .split("\"")
            .collect::<Vec<&str>>()
            .join("")
            .to_string()
    }
}

// pub trait DatabaseT {
//     fn createTable(name: String, columns: Vec<Column>) -> String;
//     fn databaseType(columnType: String) -> String;
//     fn formatQuery(query: String) -> String;

//     fn createConnection(url: String) -> Result<(), Error>;
//     fn getConnection() -> Arc<Mutex<Option<Connection>>>;
//     fn execute(sql: String, params: &[String]) -> Result<(), Error>;
// }
