use futures::Stream;
use std::pin::Pin;

mod data;
mod databases;
mod error;
mod helpers;
mod planner;
mod query;
mod traits;
mod value;

pub use data::*;
pub use databases::*;
pub use error::*;
pub use helpers::*;
pub use planner::*;
pub use query::*;
pub use traits::*;
pub use value::*;

pub type DinocoResult<T> = Result<T, DinocoError>;
pub type DinocoStream<T> = Pin<Box<dyn Stream<Item = DinocoResult<T>> + Send>>;
