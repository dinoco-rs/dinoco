use std::collections::HashMap;
use std::sync::Arc;

use crate::{DatabaseError, DinocoValue};

/// The statement behind one observed operation, as Dinoco compiled it for the
/// client's backend.
#[derive(Debug, Clone)]
pub struct ExecutedQuery {
    pub table: &'static str,
    pub sql: String,
    pub params: Vec<DinocoValue>,
    /// `true` when the operation ran through a `transaction(...)` context.
    pub in_transaction: bool,
}

/// Receives the inserted rows (field name to value, one JSON object per row),
/// or `None` when the insert failed.
pub type InsertHook = Arc<dyn Fn(Option<&[serde_json::Value]>, &ExecutedQuery, Option<&DatabaseError>) + Send + Sync>;

/// Receives how many rows were affected (update/delete) or returned (find),
/// or `None` when the operation failed.
pub type RowCountHook = Arc<dyn Fn(Option<usize>, &ExecutedQuery, Option<&DatabaseError>) + Send + Sync>;

/// Callbacks for one table, run after each operation on it.
#[derive(Clone, Default)]
pub struct TableHooks {
    pub on_insert: Option<InsertHook>,
    pub on_update: Option<RowCountHook>,
    pub on_delete: Option<RowCountHook>,
    pub on_find: Option<RowCountHook>,
}

impl TableHooks {
    pub fn is_empty(&self) -> bool {
        self.on_insert.is_none() && self.on_update.is_none() && self.on_delete.is_none() && self.on_find.is_none()
    }
}

impl std::fmt::Debug for TableHooks {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("TableHooks")
            .field("on_insert", &self.on_insert.is_some())
            .field("on_update", &self.on_update.is_some())
            .field("on_delete", &self.on_delete.is_some())
            .field("on_find", &self.on_find.is_some())
            .finish()
    }
}

/// Every table's callbacks on one client. Installed with
/// `dinoco::setup_test_methods::<M>(&client)`.
#[derive(Clone, Default, Debug)]
pub struct QueryHooks {
    tables: HashMap<&'static str, TableHooks>,
}

impl QueryHooks {
    pub fn table(&self, table: &str) -> Option<&TableHooks> {
        self.tables.get(table)
    }

    /// Replaces the callbacks of `table`; empty hooks remove the entry.
    pub fn set_table(&mut self, table: &'static str, hooks: TableHooks) {
        if hooks.is_empty() {
            self.tables.remove(table);
        } else {
            self.tables.insert(table, hooks);
        }
    }

    pub fn is_empty(&self) -> bool {
        self.tables.is_empty()
    }
}

impl DinocoValue {
    /// A JSON view of the value, as reported to query hooks.
    pub fn to_json(&self) -> serde_json::Value {
        match self {
            DinocoValue::Null => serde_json::Value::Null,
            DinocoValue::Integer(value) => (*value).into(),
            DinocoValue::Float(value) => (*value).into(),
            DinocoValue::String(value) | DinocoValue::Enum(_, value) => value.clone().into(),
            DinocoValue::Boolean(value) => (*value).into(),
            DinocoValue::Bytes(value) => value.clone().into(),
            DinocoValue::Json(value) => value.clone(),
            DinocoValue::DateTime(value) => value.to_rfc3339().into(),
            DinocoValue::Date(value) => value.to_string().into(),
        }
    }
}
