use std::vec;

use crate::DinocoValue;

#[derive(Debug, Clone)]
pub enum FindOrderBy {
    Asc(&'static str),
    Desc(&'static str),
}

#[derive(Debug, Clone)]
pub struct FindQuery {
    pub fields: &'static [&'static str],
    pub from: &'static str,

    pub conditions: Vec<FindWhere>,

    pub limit: i32,
    pub skip: i32,

    pub order_by: Option<FindOrderBy>,
    // pub relations: Vec<EntityRelation>,
}

#[derive(Debug, Clone)]
pub struct InsertQuery {
    pub table: &'static str,
    pub fields: Vec<&'static str>,
    pub rows: Vec<Vec<DinocoValue>>,
    pub returning: Option<&'static [&'static str]>,
}

#[derive(Debug, Clone)]
pub struct UpdateQuery {
    pub table: &'static str,
    pub sets: Vec<UpdateSet>,
    pub conditions: Vec<FindWhere>,
    pub returning: Option<&'static [&'static str]>,
}

#[derive(Debug, Clone)]
pub struct UpdateSet {
    pub field: &'static str,
    pub value: DinocoValue,
    pub operation: UpdateOperation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateOperation {
    Set,
    Connect,
    Disconnect,
}

#[derive(Debug, Clone)]
pub struct DeleteQuery {
    pub table: &'static str,
    pub conditions: Vec<FindWhere>,
    pub returning: Option<&'static [&'static str]>,
}

#[derive(Debug, Clone)]
pub struct CountQuery {
    pub table: &'static str,
    pub conditions: Vec<FindWhere>,
}

#[derive(Debug, Clone)]
pub struct RelationCountQuery {
    pub parent_table: &'static str,
    pub child_table: &'static str,
    pub parent_field: &'static str,
    pub child_field: &'static str,
    pub parent_conditions: Vec<FindWhere>,
    pub child_conditions: Vec<FindWhere>,
}

#[derive(Debug, Clone)]
pub struct RelationJoinQuery {
    pub query: FindQuery,
    pub parent_table: &'static str,
    pub child_table: &'static str,
    pub parent_field: &'static str,
    pub child_field: &'static str,
    pub key_count: usize,
}

#[derive(Debug, Clone)]
pub struct RelationBatchQuery {
    pub query: FindQuery,
    pub relation_key_field: &'static str,
}

#[derive(Debug, Clone)]
pub struct CreateTableMigration {
    pub table: String,
    pub columns: Vec<MigrationColumn>,
    pub foreign_keys: Vec<MigrationForeignKey>,
    pub if_not_exists: bool,
}

#[derive(Debug, Clone)]
pub struct DropTableMigration {
    pub table: String,
    pub if_exists: bool,
}

#[derive(Debug, Clone)]
pub struct AddColumnMigration {
    pub table: String,
    pub column: MigrationColumn,
}

#[derive(Debug, Clone)]
pub struct DropColumnMigration {
    pub table: String,
    pub column: String,
}

#[derive(Debug, Clone)]
pub struct AlterColumnMigration {
    pub table: String,
    pub current: MigrationColumn,
    pub desired: MigrationColumn,
}

#[derive(Debug, Clone)]
pub struct RenameColumnMigration {
    pub table: String,
    pub from: String,
    pub to: String,
}

#[derive(Debug, Clone)]
pub struct AddForeignKeyMigration {
    pub table: String,
    pub foreign_key: MigrationForeignKey,
}

#[derive(Debug, Clone)]
pub struct DropForeignKeyMigration {
    pub table: String,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct CreateIndexMigration {
    pub table: String,
    pub index: MigrationIndex,
}

#[derive(Debug, Clone)]
pub struct DropIndexMigration {
    pub table: String,
    pub index: MigrationIndex,
}

#[derive(Debug, Clone)]
pub struct CreateEnumMigration {
    pub name: String,
    pub values: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct DropEnumMigration {
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct AlterEnumMigration {
    pub name: String,
    pub current_values: Vec<String>,
    pub desired_values: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MigrationColumn {
    pub name: String,
    pub ty: MigrationColumnType,
    pub primary_key: bool,
    #[serde(default)]
    pub unique: bool,
    pub nullable: bool,
    pub default: Option<MigrationDefault>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct MigrationForeignKey {
    pub name: String,
    pub columns: Vec<String>,
    pub references_table: String,
    pub references_columns: Vec<String>,
    pub on_update: ReferentialAction,
    pub on_delete: ReferentialAction,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct MigrationIndex {
    pub name: String,
    pub columns: Vec<String>,
    #[serde(default)]
    pub automatic: bool,
    #[serde(default)]
    pub kind: MigrationIndexKind,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum MigrationIndexKind {
    #[default]
    Standard,
    Unique,
    FullText,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ReferentialAction {
    Cascade,
    Restrict,
    NoAction,
    SetNull,
    SetDefault,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum MigrationColumnType {
    String,
    Boolean,
    Integer,
    Float,
    Text,
    DateTime,
    Date,
    Json,
    Enum { name: String, values: Vec<String> },
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum MigrationDefault {
    String(String),
    Boolean(bool),
    Integer(i64),
    Float(f64),
    CurrentTimestamp,
    AutoIncrement,
}

#[derive(Debug, Clone)]
pub enum FindWhere {
    Eq(&'static str, DinocoValue),
    Neq(&'static str, DinocoValue),

    Gt(&'static str, DinocoValue),
    Gte(&'static str, DinocoValue),
    Lt(&'static str, DinocoValue),
    Lte(&'static str, DinocoValue),
    Like(&'static str, DinocoValue),
    FullText(&'static [&'static str], DinocoValue),
    Between(&'static str, DinocoValue, DinocoValue),

    Batch(&'static str, Vec<DinocoValue>),

    Null(&'static str),
    NotNull(&'static str),

    And(Vec<FindWhere>),
    Or(Vec<FindWhere>),
    Not(Box<FindWhere>),
}

#[derive(Debug, Clone, Copy, Default)]
pub struct WhereComplex;

impl WhereComplex {
    pub fn and<I>(self, conditions: I) -> FindWhere
    where
        I: IntoIterator<Item = FindWhere>,
    {
        FindWhere::And(conditions.into_iter().collect())
    }

    pub fn or(self, left: FindWhere, right: FindWhere) -> FindWhere {
        FindWhere::Or(vec![left, right])
    }

    pub fn or_many<I>(self, conditions: I) -> FindWhere
    where
        I: IntoIterator<Item = FindWhere>,
    {
        FindWhere::Or(conditions.into_iter().collect())
    }

    pub fn not(self, condition: FindWhere) -> FindWhere {
        FindWhere::Not(Box::new(condition))
    }
}

impl FindQuery {
    pub fn new(fields: &'static [&'static str], from: &'static str, limit: i32, skip: i32) -> Self {
        Self {
            fields,
            from,

            // relations: vec![],
            conditions: vec![],
            limit,
            skip,
            order_by: None,
        }
    }
}
