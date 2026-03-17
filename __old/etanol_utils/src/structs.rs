use crate::EtanolError;

use std::env;

use dotenv::dotenv;

#[derive(Clone)]
pub struct Configs(pub Vec<Config>);

#[derive(Clone)]
pub struct Config {
    pub key: String,
    pub value: String,
    pub isEnv: bool,
    pub envValue: Option<String>,
}

impl Config {
    pub fn value(&self) -> String {
        if self.isEnv {
            self.envValue.clone().unwrap()
        } else {
            self.value.clone()
        }
    }
}

impl Configs {
    pub fn new() -> Configs {
        Configs(Vec::new())
    }

    pub fn take(&self, key: String) -> Option<Config> {
        for config in self.0.iter() {
            if config.key == key {
                return Some(config.clone());
            }
        }

        None
    }
}

pub struct Env {}

impl Env {
    pub fn take(key: String) -> String {
        dotenv().ok();

        match env::var(&key) {
            Ok(value) => value,
            _ => {
                EtanolError::new(
                    format!("Cannot find '{}' in env", key),
                    "NotHaveInEnv".to_string(),
                );

                "".to_string()
            }
        }
    }
}
