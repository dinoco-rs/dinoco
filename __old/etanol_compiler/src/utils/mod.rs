mod data;
mod methods;

pub use data::*;
pub use methods::*;

pub struct Config {
    pub keywords: Vec<String>,
    pub functions: Vec<String>,
    pub config_keys: Vec<String>,
    pub types: Vec<String>,
    pub decorators: Vec<Decorator>,
}

pub struct Decorator {
    pub name: String,
    pub is_function: bool,
}

impl Decorator {
    pub fn new(name: String, is_function: bool) -> Self {
        Self { name, is_function }
    }
}

pub trait DecoratorMethods {
    fn exists(&self, name: String) -> bool;
    fn is_function(&self, name: String) -> bool;
}

impl DecoratorMethods for Vec<Decorator> {
    fn exists(&self, name: String) -> bool {
        self.iter().find(|x| x.name == name).is_some()
    }

    fn is_function(&self, name: String) -> bool {
        self.iter().find(|x| x.name == name).unwrap().is_function
    }
}

impl Config {
    pub fn new() -> Self {
        fn to_string(data: Vec<&str>) -> Vec<String> {
            data.iter().map(|&x| x.to_string()).collect()
        }

        Self {
            keywords: to_string(vec!["table", "config"]),
            functions: to_string(vec!["env", "include_file"]),
            config_keys: to_string(vec!["database", "path", "url", "include"]),
            types: to_string(vec!["String", "Number", "Boolean"]),
            decorators: vec![
                Decorator::new("id".to_string(), false),
                Decorator::new("uuid".to_string(), false),
                Decorator::new("autoincrement".to_string(), false),
                Decorator::new("default".to_string(), true),
            ],
        }
    }
}
