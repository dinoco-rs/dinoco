#![allow(non_snake_case)]

mod methods;

use uuid::Uuid;

pub use etanol_databases::{Database, ModelWhere};
pub use etanol_utils::Env;

pub use methods::*;

pub fn uuid() -> String {
    Uuid::new_v4().to_string()
}
