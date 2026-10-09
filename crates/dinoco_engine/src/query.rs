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

impl UpdateQuery {
    /// Builds a stable post-update lookup for adapters without `RETURNING`.
    /// A known `id` is always preferred. Otherwise predicates on changed
    /// fields are replaced by exact values from `set(...)` operations, while
    /// unaffected predicates remain available to narrow the lookup.
    pub fn post_update_reload_conditions(&self) -> Vec<FindWhere> {
        if let Some(id) = find_equality_value(&self.conditions, "id") {
            return vec![FindWhere::Eq("id", id)];
        }

        let mut conditions = self
            .conditions
            .iter()
            .filter(|condition| !condition_references_updated_field(condition, &self.sets))
            .cloned()
            .collect::<Vec<_>>();
        conditions.extend(
            self.sets
                .iter()
                .filter(|set| set.operation == UpdateOperation::Set)
                .map(|set| FindWhere::Eq(set.field, set.value.clone())),
        );

        if conditions.is_empty() { self.conditions.clone() } else { conditions }
    }
}

fn find_equality_value(conditions: &[FindWhere], field: &'static str) -> Option<DinocoValue> {
    conditions.iter().find_map(|condition| match condition {
        FindWhere::Eq(candidate, value) if *candidate == field => Some(value.clone()),
        FindWhere::And(conditions) | FindWhere::Or(conditions) => find_equality_value(conditions, field),
        FindWhere::Not(condition) => find_equality_value(std::slice::from_ref(condition.as_ref()), field),
        _ => None,
    })
}

fn condition_references_updated_field(condition: &FindWhere, sets: &[UpdateSet]) -> bool {
    let updated = |field: &'static str| sets.iter().any(|set| set.field == field);
    match condition {
        FindWhere::Eq(field, _)
        | FindWhere::Neq(field, _)
        | FindWhere::Gt(field, _)
        | FindWhere::Gte(field, _)
        | FindWhere::Lt(field, _)
        | FindWhere::Lte(field, _)
        | FindWhere::Like(field, _)
        | FindWhere::Between(field, _, _)
        | FindWhere::Batch(field, _)
        | FindWhere::Null(field)
        | FindWhere::NotNull(field) => updated(field),
        FindWhere::FullText(fields, _) => fields.iter().any(|field| updated(field)),
        FindWhere::ManyToMany(match_) => updated(match_.local_key),
        // The nested conditions are about the related table, which an
        // `UPDATE` of this table never changes; only the link column can move.
        FindWhere::Relation(relation) => updated(relation.parent_field),
        FindWhere::And(conditions) | FindWhere::Or(conditions) => {
            conditions.iter().any(|condition| condition_references_updated_field(condition, sets))
        }
        FindWhere::Not(condition) => condition_references_updated_field(condition, sets),
    }
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
    Increment,
    Decrement,
    Multiply,
    Divide,
    Connect,
    Disconnect,
    ConnectManyToMany(ManyToManyUpdate),
    DisconnectManyToMany(ManyToManyUpdate),
    /// Assigns the database's current UTC timestamp (`@updated_at` on a
    /// `DateTime` field). Binds no parameter: the value comes from the server.
    CurrentTimestamp,
    /// Assigns the database's current UTC date (`@updated_at` on a `Date`
    /// field). Binds no parameter.
    CurrentDate,
}

/// The SQL each dialect evaluates to "now" when assigning
/// [`UpdateOperation::CurrentTimestamp`]/[`UpdateOperation::CurrentDate`].
/// Both are UTC so they match the values Dinoco itself writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CurrentTimeSql {
    pub timestamp: &'static str,
    pub date: &'static str,
}

impl UpdateOperation {
    pub fn is_scalar(self) -> bool {
        matches!(
            self,
            Self::Set
                | Self::Increment
                | Self::Decrement
                | Self::Multiply
                | Self::Divide
                | Self::CurrentTimestamp
                | Self::CurrentDate
        )
    }

    /// Whether the assignment consumes the set's value as a bind parameter.
    pub fn binds_value(self) -> bool {
        self.is_scalar() && !matches!(self, Self::CurrentTimestamp | Self::CurrentDate)
    }

    /// Builds the right-hand side of a scalar assignment. Identifiers,
    /// placeholders and the current-time expressions are supplied by the
    /// active dialect and values remain bind parameters. `placeholder` is
    /// ignored by operations that do not [bind a value](Self::binds_value).
    pub fn assignment_sql(self, field: &str, placeholder: &str, now: CurrentTimeSql) -> Option<String> {
        match self {
            Self::Set => Some(format!("{field} = {placeholder}")),
            Self::Increment => Some(format!("{field} = {field} + {placeholder}")),
            Self::Decrement => Some(format!("{field} = {field} - {placeholder}")),
            Self::Multiply => Some(format!("{field} = {field} * {placeholder}")),
            Self::Divide => Some(format!("{field} = {field} / {placeholder}")),
            Self::CurrentTimestamp => Some(format!("{field} = {}", now.timestamp)),
            Self::CurrentDate => Some(format!("{field} = {}", now.date)),
            Self::Connect | Self::Disconnect | Self::ConnectManyToMany(_) | Self::DisconnectManyToMany(_) => None,
        }
    }
}

/// A column the database refreshes on every scalar `UPDATE` of its row
/// (`@updated_at` in the schema), unless the update sets it explicitly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UpdatedAtField {
    pub name: &'static str,
    /// [`UpdateOperation::CurrentTimestamp`] or [`UpdateOperation::CurrentDate`].
    pub operation: UpdateOperation,
}

impl UpdatedAtField {
    pub const fn timestamp(name: &'static str) -> Self {
        Self { name, operation: UpdateOperation::CurrentTimestamp }
    }

    pub const fn date(name: &'static str) -> Self {
        Self { name, operation: UpdateOperation::CurrentDate }
    }

    pub fn update_set(self) -> UpdateSet {
        UpdateSet { field: self.name, value: DinocoValue::Null, operation: self.operation }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ManyToManyUpdate {
    pub join_table: &'static str,
    pub parent_field: &'static str,
    pub join_parent_field: &'static str,
    pub join_child_field: &'static str,
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
pub struct ExistsQuery {
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
#[doc(hidden)]
pub struct RelationOccurrenceQuery {
    pub query: FindQuery,
    pub child_field: &'static str,
    pub key_count: usize,
}

#[derive(Debug, Clone)]
pub struct ManyToManyRelationQuery {
    pub query: FindQuery,
    pub join_table: &'static str,
    pub parent_field: &'static str,
    pub child_field: &'static str,
    pub join_parent_field: &'static str,
    pub join_child_field: &'static str,
    pub key_count: usize,
}

#[derive(Debug, Clone)]
pub struct ManyToManyRelationCountQuery {
    pub parent_table: &'static str,
    pub child_table: &'static str,
    pub join_table: &'static str,
    pub parent_field: &'static str,
    pub child_field: &'static str,
    pub join_parent_field: &'static str,
    pub join_child_field: &'static str,
    pub parent_conditions: Vec<FindWhere>,
    pub child_conditions: Vec<FindWhere>,
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
pub struct RenameTableMigration {
    pub from: String,
    pub to: String,
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

    /// Membership test against a many-to-many join table. Produced by the
    /// generated virtual `Option<PrimaryKey>` fields so a caller can filter a
    /// side of a many-to-many relation by the id of a row on the other side.
    ManyToMany(ManyToManyMatch),

    /// Filter on a related model, compiled to a correlated `[NOT] EXISTS`
    /// subquery. Produced by the generated relation fields of `Where` types,
    /// so a query can be narrowed by its related rows without loading them.
    Relation(RelationMatch),

    And(Vec<FindWhere>),
    Or(Vec<FindWhere>),
    Not(Box<FindWhere>),
}

/// Payload for [`FindWhere::ManyToMany`].
///
/// Compiles to `<local_key> IN (SELECT <join_local_field> FROM <join_table>
/// WHERE <predicate>)`, where `predicate` is any [`FindWhere`] built against
/// `join_target_field`. Rendered as `NOT IN` when `negated` is set.
#[derive(Debug, Clone)]
pub struct ManyToManyMatch {
    /// Column on the queried entity's own table (its primary/reference key).
    pub local_key: &'static str,
    /// Join table that connects the two sides of the relation.
    pub join_table: &'static str,
    /// Join-table column that references the queried entity.
    pub join_local_field: &'static str,
    /// Join-table column that references the related entity; the field every
    /// `predicate` condition is expressed against.
    pub join_target_field: &'static str,
    /// `true` renders `NOT IN`, keeping rows that do *not* match the predicate
    /// (including rows with no link at all).
    pub negated: bool,
    /// Condition applied to `join_target_field` inside the subquery.
    pub predicate: Box<FindWhere>,
}

/// Payload for [`FindWhere::Relation`].
///
/// Compiles to `EXISTS (SELECT 1 FROM <child_table> AS <alias> WHERE
/// <alias>.<child_field> = <outer>.<parent_field> AND <conditions>)`, negated
/// as the [`quantifier`](Self::quantifier) requires. A many-to-many relation
/// reaches the related table through its join table instead. `<outer>` is the
/// enclosing query's qualifier, or `parent_table` when it has none. Every
/// subquery aliases its tables by nesting depth, so self relations and nested
/// relation filters never shadow the row they correlate with.
#[derive(Debug, Clone)]
pub struct RelationMatch {
    /// Table of the entity being filtered.
    pub parent_table: &'static str,
    /// Column on `parent_table` the relation is matched on.
    pub parent_field: &'static str,
    /// Table of the related entity.
    pub child_table: &'static str,
    /// Column on `child_table` matched against `parent_field` (against the
    /// join table's `child_field` for a many-to-many relation).
    pub child_field: &'static str,
    /// Join table of a many-to-many relation.
    pub join: Option<RelationJoinTable>,
    pub quantifier: RelationQuantifier,
    /// Conditions on the related entity, combined with `AND`. Empty matches
    /// every related row.
    pub conditions: Vec<FindWhere>,
}

/// Join table a many-to-many [`RelationMatch`] goes through.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RelationJoinTable {
    pub table: &'static str,
    /// Join-table column that references the parent's `parent_field`.
    pub parent_field: &'static str,
    /// Join-table column that references the related table's `child_field`.
    pub child_field: &'static str,
}

/// How many related rows must match a [`RelationMatch`]'s conditions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelationQuantifier {
    /// At least one related row matches (`EXISTS`).
    Some,
    /// No related row matches (`NOT EXISTS`).
    None,
    /// Every related row matches, vacuously true when there is none (`NOT
    /// EXISTS` over the related rows whose conditions aren't `TRUE`, so a
    /// condition that evaluates to `NULL` counts as not matching).
    Every,
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
