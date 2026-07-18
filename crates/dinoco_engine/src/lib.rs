mod backends;
mod query;
mod traits;
mod value;

pub use backends::*;
pub use query::*;
pub use traits::*;
pub use value::*;

pub use rusqlite::Row as SqliteRow;

pub struct DinocoClient {
    pub backend: Backend,
}
