use crate::{FindWhere, RelationMatch, RelationQuantifier};

/// Alias of the related table in a relation filter subquery, suffixed with the
/// nesting depth. A nested relation filter is compiled with its parent's alias
/// as qualifier, which is how the next depth is found.
const CHILD_ALIAS_PREFIX: &str = "__dinoco_relation_";
const JOIN_ALIAS_PREFIX: &str = "__dinoco_relation_join_";
const KEYS_ALIAS_PREFIX: &str = "__dinoco_relation_keys_";

/// The dialect-independent shape of a [`RelationMatch`] subquery. Each SQL
/// compiler renders the nested conditions against [`alias`](Self::alias) with
/// its own placeholders, then hands them to [`finish`](Self::finish).
pub(crate) struct RelationSubquery {
    depth: usize,
    alias: String,
    source: String,
    /// Column of the subquery correlated with the outer row.
    key: String,
    /// Column of the outer row.
    outer: String,
}

impl RelationSubquery {
    /// `qualifier` is the enclosing query's qualifier and `identifier` the
    /// dialect's identifier quoting.
    pub(crate) fn new(relation: &RelationMatch, qualifier: Option<&str>, identifier: fn(&str) -> String) -> Self {
        let depth = qualifier
            .and_then(|qualifier| qualifier.strip_prefix(CHILD_ALIAS_PREFIX))
            .and_then(|depth| depth.parse::<usize>().ok())
            .unwrap_or(0)
            + 1;
        let alias = format!("{CHILD_ALIAS_PREFIX}{depth}");
        let field = |table: &str, field: &str| format!("{}.{}", identifier(table), identifier(field));
        let outer = field(qualifier.unwrap_or(relation.parent_table), relation.parent_field);
        let child_key = field(&alias, relation.child_field);
        let child_source = format!("{} AS {alias}", identifier(relation.child_table));

        let (source, key) = match relation.join {
            Some(join) => {
                let join_alias = format!("{JOIN_ALIAS_PREFIX}{depth}");
                (
                    format!(
                        "{} AS {join_alias} INNER JOIN {child_source} ON {child_key} = {}",
                        identifier(join.table),
                        field(&join_alias, join.child_field),
                    ),
                    field(&join_alias, join.parent_field),
                )
            }
            None => (child_source, child_key),
        };

        Self { depth, alias, source, key, outer }
    }

    /// Qualifier for the conditions on the related table.
    pub(crate) fn alias(&self) -> &str {
        &self.alias
    }

    /// Builds the correlated `[NOT] EXISTS (...)` expression from the rendered
    /// conditions.
    pub(crate) fn finish(self, quantifier: RelationQuantifier, conditions: &[String]) -> String {
        let Some(filter) = related_row_filter(quantifier, conditions) else {
            return "1 = 1".to_string();
        };
        let Self { source, key, outer, .. } = self;
        let filter = filter.map(|filter| format!(" AND {filter}")).unwrap_or_default();

        format!("{}EXISTS (SELECT 1 FROM {source} WHERE {key} = {outer}{filter})", negation(quantifier))
    }

    /// [`finish`](Self::finish) for a MySQL `UPDATE`/`DELETE` whose filter reads
    /// the table being written: MySQL only allows that read from a derived
    /// table it materializes before writing (error 1093 otherwise). The
    /// derived table holds the distinct keys of the matching related rows —
    /// `DISTINCT` also keeps MySQL from merging it back into the subquery —
    /// and the correlation stays outside of it, so NULL keys behave exactly as
    /// in [`finish`](Self::finish).
    pub(crate) fn finish_materialized(self, quantifier: RelationQuantifier, conditions: &[String]) -> String {
        let Some(filter) = related_row_filter(quantifier, conditions) else {
            return "1 = 1".to_string();
        };
        let Self { depth, source, key, outer, .. } = self;
        let keys = format!("{KEYS_ALIAS_PREFIX}{depth}");
        let filter = filter.map(|filter| format!(" WHERE {filter}")).unwrap_or_default();

        format!(
            "{}EXISTS (SELECT 1 FROM (SELECT DISTINCT {key} AS __dinoco_key FROM {source}{filter}) AS {keys} WHERE {keys}.__dinoco_key = {outer})",
            negation(quantifier),
        )
    }
}

/// The condition a related row is checked against inside the subquery, or
/// `None` when the relation filter is trivially true.
fn related_row_filter(quantifier: RelationQuantifier, conditions: &[String]) -> Option<Option<String>> {
    match (quantifier, conditions.is_empty()) {
        // Every related row satisfies an empty condition list.
        (RelationQuantifier::Every, true) => None,
        (_, true) => Some(None),
        // `IS NOT TRUE` also catches conditions that evaluate to NULL, which a
        // plain `NOT (...)` would let through as matching.
        (RelationQuantifier::Every, false) => Some(Some(format!("({}) IS NOT TRUE", conditions.join(" AND ")))),
        (_, false) => Some(Some(conditions.join(" AND "))),
    }
}

fn negation(quantifier: RelationQuantifier) -> &'static str {
    if quantifier == RelationQuantifier::Some { "" } else { "NOT " }
}

/// Whether compiling `relation` reads `table`, at any nesting depth.
pub(crate) fn relation_reads_table(relation: &RelationMatch, table: &str) -> bool {
    relation.child_table == table
        || relation.join.is_some_and(|join| join.table == table)
        || relation.conditions.iter().any(|condition| condition_reads_table(condition, table))
}

fn condition_reads_table(condition: &FindWhere, table: &str) -> bool {
    match condition {
        FindWhere::Relation(relation) => relation_reads_table(relation, table),
        FindWhere::ManyToMany(match_) => match_.join_table == table,
        FindWhere::And(conditions) | FindWhere::Or(conditions) => {
            conditions.iter().any(|condition| condition_reads_table(condition, table))
        }
        FindWhere::Not(condition) => condition_reads_table(condition, table),
        _ => false,
    }
}
