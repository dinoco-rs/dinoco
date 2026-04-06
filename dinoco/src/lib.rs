extern crate self as dinoco;

mod data;
mod execution;
mod fields;
mod ids;
mod methods;
mod model;

pub use dinoco_derives::{Extend, Rowable};
pub use dinoco_engine::{
    DinocoAdapter, DinocoClient, DinocoError, DinocoGenericRow, DinocoResult, DinocoRow, DinocoValue, OrderDirection,
};
pub use uuid::Uuid;

pub use chrono::{DateTime as DateTimeUtc, NaiveDate, Utc};
pub use futures;
pub use serde_json::Value as JsonValue;

pub use data::{IncludeNode, OrderBy, ReadMode};
pub use execution::{execute_delete, execute_first, execute_insert, execute_many, execute_update, execute_update_many};
pub use fields::{RelationField, RelationQuery, ScalarField};
pub use ids::{snowflake, uuid_v7};
pub use methods::{
    Delete, DeleteMany, FindFirst, FindMany, Insert, InsertMany, Update, UpdateMany, delete, delete_many, find_first,
    find_many, insert_into, insert_many, update, update_many,
};
pub use model::{
    IncludeApplier, IncludeLoaderFuture, InsertModel, InsertRelation, IntoDinocoValue, IntoIncludeNode, Model,
    Projection, ScalarFieldValue, UpdateModel,
};
