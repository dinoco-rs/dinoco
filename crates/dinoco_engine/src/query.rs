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
pub enum FindWhere {
    Eq(&'static str, DinocoValue),
    Neq(&'static str, DinocoValue),

    Gt(&'static str, DinocoValue),
    Gte(&'static str, DinocoValue),
    Lt(&'static str, DinocoValue),
    Lte(&'static str, DinocoValue),

    Batch(&'static str, Vec<DinocoValue>),

    Null(&'static str),
    NotNull(&'static str),
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
