mod aggregate;
mod count;
mod error;
mod fields;
mod hooks;
mod insert;
mod methods;
mod order_by;
mod projection;
mod read;
mod relation;
mod seed;
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
#[cfg(feature = "cli")]
pub mod codegen {
    pub use dinoco_codegen::prelude;
    pub use dinoco_codegen::prelude::*;
}

pub use anyhow;
pub use async_trait::async_trait;
pub use dinoco_derives::{DinocoEnum, DinocoRuntimeEnum, Entity, EntityExtend, Extend};
pub use dinoco_engine::*;
#[doc(hidden)]
pub use dinoco_engine::{__dinoco_mysql, __dinoco_pluck_column, __dinoco_postgres, __dinoco_sqlite};
pub use serde;

pub use aggregate::*;
pub use count::*;
pub use error::*;
pub use fields::*;
pub(crate) use hooks::*;
pub use insert::*;
pub use methods::*;
pub use order_by::*;
pub(crate) use projection::*;
pub use read::*;
pub use relation::*;
pub use seed::__run_seed;
pub use testing::*;
pub use transaction::*;
pub use update::*;
