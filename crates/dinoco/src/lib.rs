mod count;
mod error;
mod fields;
mod hooks;
mod insert;
mod methods;
mod order_by;
mod read;
mod relation;
mod testing;
mod transaction;
mod update;

/// Everything `dinoco/transform.rs` uses — `DinocoTransformer`, `Schema`,
/// `Model`, `Enum`, `Field`, `Variant`, `Relation`, `ImplBlock`, `ImplMethod`,
/// `Visibility`, `Receiver`, `RustType` and the `quote!` macro — so a project
/// only depends on `dinoco`:
///
/// ```ignore
/// use dinoco::codegen::*;
/// ```
pub mod codegen {
    pub use dinoco_codegen::prelude;
    pub use dinoco_codegen::prelude::*;
}

pub use anyhow;
pub use async_trait::async_trait;
pub use dinoco_derives::{DinocoEnum, Entity, EntityExtend, Extend, dinoco};
pub use dinoco_engine::*;
pub use serde;

pub use count::*;
pub use error::*;
pub use fields::*;
pub(crate) use hooks::*;
pub use insert::*;
pub use methods::*;
pub use order_by::*;
pub use read::*;
pub use relation::*;
pub use testing::*;
pub use transaction::*;
pub use update::*;
