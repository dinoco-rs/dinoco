extern crate self as dinoco;

mod cache;
mod data;
mod execution;
mod fields;
mod ids;
mod methods;
mod model;

pub use dinoco_derives::{Extend, Rowable};
pub use dinoco_engine::{
    AdapterDialect, DinocoAdapter, DinocoClient, DinocoClientConfig, DinocoError, DinocoGenericRow, DinocoQueryLog,
    DinocoQueryLogWriter, DinocoQueryLogger, DinocoQueryLoggerOptions, DinocoRedisConfig, DinocoResult, DinocoRow,
    DinocoValue, Expression, MySqlAdapter, OrderDirection, PostgresAdapter, QueryBuilder, SelectStatement,
    SqliteAdapter,
};
pub use uuid::Uuid;

pub use chrono::{DateTime as DateTimeUtc, NaiveDate, Utc};
pub use futures;
pub use serde;
pub use serde_json::Value as JsonValue;

pub use cache::{CachePolicy, CachedFindFirst, CachedFindMany, DinocoCache};
pub use data::{CountNode, IncludeNode, OrderBy, ReadMode};
pub use execution::{
    execute_connection_updates, execute_count, execute_delete, execute_find_and_update, execute_first, execute_insert,
    execute_insert_connected_payload, execute_insert_connected_payloads, execute_insert_payload,
    execute_insert_payload_returning, execute_insert_related_payload, execute_insert_related_payloads,
    execute_insert_relation_links, execute_insert_returning, execute_many, execute_relation_writes, execute_update,
    execute_update_many, execute_update_many_returning, execute_update_returning, qualify_expression,
    qualify_query_column, qualify_select_statement,
};
pub use fields::{
    FieldUpdate, RelationField, RelationMutationWhere, RelationQuery, RelationScalarField, ScalarField, UpdateField,
};
pub use ids::{snowflake, uuid_v7};
pub use methods::{
    Count, Delete, DeleteMany, FindAndUpdate, FindFirst, FindMany, Insert, InsertMany, Update, UpdateMany, count,
    delete, delete_many, find_and_update, find_first, find_many, insert_into, insert_many, update, update_many,
};
pub use model::{
    ConnectionUpdatePlan, FindAndUpdateModel, IncludeApplier, IncludeLoaderFuture, InsertConnection,
    InsertConnectionPayload, InsertModel, InsertNested, InsertPayload, InsertRelation, IntoCountNode, IntoDinocoValue,
    IntoIncludeNode, Model, Projection, RelationLinkPlan, RelationMutationModel, RelationMutationTarget,
    RelationWriteAction, RelationWritePlan, ScalarFieldValue, UpdateModel,
};
