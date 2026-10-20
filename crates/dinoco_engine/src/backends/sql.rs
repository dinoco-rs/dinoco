//! The SQL compiler shared by every backend.
//!
//! Queries are written straight into one pre-sized buffer by [`SqlWriter`],
//! which also numbers placeholders and collects the bound values in the order
//! they appear in the statement. The few places where the databases disagree
//! (placeholders, identifier quoting, "now", full-text search, CTE syntax and
//! MySQL's restrictions on writes) are decided by [`Dialect`].

use crate::{
    AggregateColumn, AggregateFunction, AggregateGroup, AggregateQuery, CountQuery, CurrentTimeSql, DateBucket,
    DeleteQuery, DinocoValue, ExistsQuery, FindOrderBy, FindQuery, FindWhere, GroupBy, GroupOrder, InsertQuery,
    LookupQuery, ManyToManyRelationCountQuery, ManyToManyRelationQuery, RelationBatchQuery, RelationCountQuery,
    RelationJoinQuery, RelationMatch, RelationOccurrenceQuery, RelationQuantifier, UpdateOperation, UpdateQuery,
    UpdateSet,
};

/// Alias of the related table in a relation filter subquery, suffixed with the
/// nesting depth. A nested relation filter is compiled with its parent's alias
/// as qualifier, which is how the next depth is found.
const CHILD_ALIAS_PREFIX: &str = "__dinoco_relation_";
const JOIN_ALIAS_PREFIX: &str = "__dinoco_relation_join_";
const KEYS_ALIAS_PREFIX: &str = "__dinoco_relation_keys_";
const PARENT_KEYS_ORDINAL: &str = "__dinoco_parent_keys.__dinoco_relation_ordinal";

/// Keywords reserved by PostgreSQL, MySQL 8.4 or SQLite, lowercase and
/// sorted for binary search. An identifier that is one of them is quoted in
/// every dialect: a lowercase name means the same quoted or not, so quoting a
/// word only another database reserves is harmless.
#[rustfmt::skip]
const RESERVED_IDENTIFIERS: [&str; 345] = [
    "abort", "accessible", "action", "add", "after", "all", "alter", "always", "analyse", "analyze", "and", "any",
    "array", "as", "asc", "asensitive", "asymmetric", "attach", "authorization", "autoincrement", "before", "begin",
    "between", "bigint", "binary", "blob", "both", "by", "call", "cascade", "case", "cast", "change", "char",
    "character", "check", "collate", "collation", "column", "commit", "concurrently", "condition", "conflict",
    "constraint", "continue", "convert", "create", "cross", "cube", "cume_dist", "current", "current_catalog",
    "current_date", "current_role", "current_schema", "current_time", "current_timestamp", "current_user", "cursor",
    "database", "databases", "day_hour", "day_microsecond", "day_minute", "day_second", "dec", "decimal", "declare",
    "default", "deferrable", "deferred", "delayed", "delete", "dense_rank", "desc", "describe", "detach",
    "deterministic", "distinct", "distinctrow", "div", "do", "double", "drop", "dual", "each", "else", "elseif",
    "empty", "enclosed", "end", "escape", "escaped", "except", "exclude", "exclusive", "exists", "exit", "explain",
    "fail", "false", "fetch", "filter", "first", "first_value", "float", "float4", "float8", "following", "for",
    "force", "foreign", "freeze", "from", "full", "fulltext", "function", "generated", "get", "glob", "grant",
    "group", "grouping", "groups", "having", "high_priority", "hour_microsecond", "hour_minute", "hour_second", "if",
    "ignore", "ilike", "immediate", "in", "index", "indexed", "infile", "initially", "inner", "inout", "insensitive",
    "insert", "instead", "int", "int1", "int2", "int3", "int4", "int8", "integer", "intersect", "interval", "into",
    "io_after_gtids", "io_before_gtids", "is", "isnull", "iterate", "join", "json_table", "key", "keys", "kill",
    "lag", "last", "last_value", "lateral", "lead", "leading", "leave", "left", "library", "like", "limit", "linear",
    "lines", "load", "localtime", "localtimestamp", "lock", "long", "longblob", "longtext", "loop", "low_priority",
    "manual", "master_bind", "master_ssl_verify_server_cert", "match", "materialized", "maxvalue", "mediumblob",
    "mediumint", "mediumtext", "middleint", "minute_microsecond", "minute_second", "mod", "modifies", "natural", "no",
    "no_write_to_binlog", "not", "nothing", "notnull", "nth_value", "ntile", "null", "nulls", "numeric", "of",
    "offset", "on", "only", "optimize", "optimizer_costs", "option", "optionally", "or", "order", "others", "out",
    "outer", "outfile", "over", "overlaps", "parallel", "partition", "percent_rank", "placing", "plan", "pragma",
    "preceding", "precision", "primary", "procedure", "purge", "qualify", "query", "raise", "range", "rank", "read",
    "read_write", "reads", "real", "recursive", "references", "regexp", "reindex", "release", "rename", "repeat",
    "replace", "require", "resignal", "restrict", "return", "returning", "revoke", "right", "rlike", "rollback",
    "row", "row_number", "rows", "savepoint", "schema", "schemas", "second_microsecond", "select", "sensitive",
    "separator", "session_user", "set", "show", "signal", "similar", "smallint", "some", "spatial", "specific", "sql",
    "sql_big_result", "sql_calc_found_rows", "sql_small_result", "sqlexception", "sqlstate", "sqlwarning", "ssl",
    "starting", "stored", "straight_join", "symmetric", "system", "system_user", "table", "tablesample", "temp",
    "temporary", "terminated", "then", "ties", "tinyblob", "tinyint", "tinytext", "to", "trailing", "transaction",
    "trigger", "true", "unbounded", "undo", "union", "unique", "unlock", "unsigned", "update", "usage", "use", "user",
    "using", "utc_date", "utc_time", "utc_timestamp", "vacuum", "values", "varbinary", "varchar", "varcharacter",
    "variadic", "varying", "verbose", "view", "virtual", "when", "where", "while", "window", "with", "without",
    "write", "xor", "year_month", "zerofill",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(not(all(feature = "sqlite", feature = "postgres", feature = "mysql")), allow(dead_code))]
pub(crate) enum Dialect {
    Sqlite,
    Postgres,
    Mysql,
}

impl Dialect {
    fn quote(self) -> char {
        match self {
            Dialect::Mysql => '`',
            Dialect::Sqlite | Dialect::Postgres => '"',
        }
    }

    /// The SQL each database evaluates to the current UTC time for
    /// `@updated_at` columns.
    pub(crate) const fn current_time(self) -> CurrentTimeSql {
        match self {
            // SQLite has no timestamp type: `DateTime` values are stored as
            // RFC 3339 text (see the adapter's `ToSql`), so `@updated_at`
            // writes the same shape.
            Dialect::Sqlite => {
                CurrentTimeSql { timestamp: "strftime('%Y-%m-%dT%H:%M:%f+00:00', 'now')", date: "date('now')" }
            }
            // `DateTime` columns are `TIMESTAMP` (without time zone) holding
            // UTC wall clock values, so "now" is taken in UTC instead of the
            // session time zone.
            Dialect::Postgres => CurrentTimeSql {
                timestamp: "(CURRENT_TIMESTAMP AT TIME ZONE 'UTC')",
                date: "(CURRENT_TIMESTAMP AT TIME ZONE 'UTC')::date",
            },
            // UTC regardless of the session time zone, matching the naive UTC
            // values the adapter binds for `DateTime`/`Date`.
            Dialect::Mysql => CurrentTimeSql { timestamp: "UTC_TIMESTAMP()", date: "UTC_DATE()" },
        }
    }

    /// `identifier`, quoted when it is reserved or not a plain lowercase name.
    pub(crate) fn identifier(self, identifier: &str) -> String {
        let mut sql = String::with_capacity(identifier.len() + 2);
        write_identifier(&mut sql, self, identifier);
        sql
    }

    /// `identifier`, always quoted.
    pub(crate) fn quoted_identifier(self, identifier: &str) -> String {
        let mut sql = String::with_capacity(identifier.len() + 2);
        write_quoted(&mut sql, self.quote(), identifier);
        sql
    }
}

/// The table (or relation alias) the fields of a condition belong to.
#[derive(Clone, Copy)]
enum Qualifier<'a> {
    None,
    Table(&'a str),
    /// Alias of the related table of a relation filter at this depth.
    Relation(usize),
}

impl Qualifier<'_> {
    /// Nesting depth of the relation filter this qualifier belongs to; `0`
    /// outside of any relation filter.
    fn relation_depth(self) -> usize {
        match self {
            Qualifier::None => 0,
            Qualifier::Table(table) => {
                table.strip_prefix(CHILD_ALIAS_PREFIX).and_then(|depth| depth.parse().ok()).unwrap_or(0)
            }
            Qualifier::Relation(depth) => depth,
        }
    }
}

pub(crate) struct SqlWriter {
    dialect: Dialect,
    sql: String,
    params: Vec<DinocoValue>,
    placeholders: usize,
}

impl SqlWriter {
    fn new(dialect: Dialect, sql_capacity: usize, params: Vec<DinocoValue>) -> Self {
        Self { dialect, sql: String::with_capacity(sql_capacity), params, placeholders: 0 }
    }

    fn finish(self) -> (String, Vec<DinocoValue>) {
        (self.sql, self.params)
    }

    fn push(&mut self, text: &str) {
        self.sql.push_str(text);
    }

    fn number(&mut self, number: usize) {
        write_number(&mut self.sql, number);
    }

    /// Writes the next placeholder without binding a value; used for values
    /// the caller binds itself (relation keys).
    fn placeholder(&mut self) {
        self.placeholders += 1;
        match self.dialect {
            Dialect::Postgres => {
                self.sql.push('$');
                write_number(&mut self.sql, self.placeholders);
            }
            Dialect::Sqlite | Dialect::Mysql => self.sql.push('?'),
        }
    }

    fn bind(&mut self, value: DinocoValue) {
        self.placeholder();
        self.params.push(value);
    }

    fn placeholders(&mut self, count: usize) {
        for index in 0..count {
            if index > 0 {
                self.push(", ");
            }
            self.placeholder();
        }
    }

    fn identifier(&mut self, identifier: &str) {
        write_identifier(&mut self.sql, self.dialect, identifier);
    }

    fn identifiers(&mut self, identifiers: &[&str]) {
        for (index, identifier) in identifiers.iter().enumerate() {
            if index > 0 {
                self.push(", ");
            }
            self.identifier(identifier);
        }
    }

    fn relation_alias(&mut self, prefix: &str, depth: usize) {
        self.push(prefix);
        self.number(depth);
    }

    fn qualifier(&mut self, qualifier: Qualifier<'_>) {
        match qualifier {
            Qualifier::None => {}
            Qualifier::Table(table) => self.identifier(table),
            Qualifier::Relation(depth) => self.relation_alias(CHILD_ALIAS_PREFIX, depth),
        }
    }

    /// `field` qualified by `qualifier`, or split on its own dots when it is
    /// already qualified.
    fn field(&mut self, field: &str, qualifier: Qualifier<'_>) {
        let dotted = field.contains('.');
        if !dotted && !matches!(qualifier, Qualifier::None) {
            self.qualifier(qualifier);
            self.sql.push('.');
            self.identifier(field);
        } else if dotted {
            for (index, part) in field.split('.').enumerate() {
                if index > 0 {
                    self.sql.push('.');
                }
                self.identifier(part);
            }
        } else {
            self.identifier(field);
        }
    }

    fn fields(&mut self, fields: &[&str], qualifier: Qualifier<'_>) {
        for (index, field) in fields.iter().enumerate() {
            if index > 0 {
                self.push(", ");
            }
            self.field(field, qualifier);
        }
    }

    /// `<table>.<field>`, each part quoted when needed. Unlike [`Self::field`],
    /// `field` is never split on dots.
    fn raw_field(&mut self, table: &str, field: &str) {
        self.identifier(table);
        self.sql.push('.');
        self.identifier(field);
    }

    fn returning(&mut self, returning: Option<&[&str]>) {
        if let Some(returning) = returning {
            self.push(" RETURNING ");
            self.identifiers(returning);
        }
    }

    fn where_clause(&mut self, conditions: Vec<FindWhere>, qualifier: Qualifier<'_>) {
        if !conditions.is_empty() {
            self.push(" WHERE ");
            self.conditions(conditions, qualifier, " AND ");
        }
    }

    fn and_clause(&mut self, conditions: Vec<FindWhere>, qualifier: Qualifier<'_>) {
        if !conditions.is_empty() {
            self.push(" AND ");
            self.conditions(conditions, qualifier, " AND ");
        }
    }

    /// The `WHERE` of an `UPDATE`/`DELETE` on `table`. MySQL rejects a write
    /// whose subquery reads the table being written (error 1093), so there
    /// relation filters that reach `table` read it from a materialized derived
    /// table instead; every other condition compiles as usual.
    fn write_where_clause(&mut self, conditions: Vec<FindWhere>, table: &str) {
        if conditions.is_empty() {
            return;
        }

        self.push(" WHERE ");
        if self.dialect == Dialect::Mysql {
            self.write_conditions(conditions, table, " AND ");
        } else {
            self.conditions(conditions, Qualifier::None, " AND ");
        }
    }

    fn find_tail(
        &mut self,
        conditions: Vec<FindWhere>,
        order_by: Option<FindOrderBy>,
        limit: i32,
        skip: i32,
        qualifier: Qualifier<'_>,
    ) {
        self.where_clause(conditions, qualifier);
        self.order_by(order_by, qualifier);
        self.page(limit, skip);
    }

    fn page(&mut self, limit: i32, skip: i32) {
        if limit >= 0 {
            self.push(" LIMIT ");
            self.bind(DinocoValue::Integer(i64::from(limit)));
        } else if skip >= 0 {
            // SQLite and MySQL only take `OFFSET` after a `LIMIT`; these are
            // their "no limit" values. PostgreSQL takes `OFFSET` alone.
            match self.dialect {
                Dialect::Sqlite => self.push(" LIMIT -1"),
                Dialect::Mysql => self.push(" LIMIT 18446744073709551615"),
                Dialect::Postgres => {}
            }
        }

        if skip >= 0 {
            self.push(" OFFSET ");
            self.bind(DinocoValue::Integer(i64::from(skip)));
        }
    }

    /// The aggregate's SQL, cast where a database widens the result: a
    /// PostgreSQL `SUM(bigint)` is `numeric` and a MySQL `SUM` is `DECIMAL`.
    fn aggregate(&mut self, aggregate: AggregateColumn) {
        let field = aggregate.field;
        let (open, close) = match (aggregate.function, self.dialect) {
            (AggregateFunction::SumInteger, Dialect::Sqlite) => ("COALESCE(SUM(", "), 0)"),
            (AggregateFunction::SumInteger, Dialect::Postgres) => ("CAST(COALESCE(SUM(", "), 0) AS BIGINT)"),
            (AggregateFunction::SumInteger, Dialect::Mysql) => ("CAST(COALESCE(SUM(", "), 0) AS SIGNED)"),
            // `TOTAL` is SQLite's `SUM` that is always a float, and `0.0` over
            // no values.
            (AggregateFunction::SumFloat, Dialect::Sqlite) => ("TOTAL(", ")"),
            (AggregateFunction::SumFloat, Dialect::Postgres) => ("CAST(COALESCE(SUM(", "), 0) AS DOUBLE PRECISION)"),
            (AggregateFunction::SumFloat, Dialect::Mysql) => ("CAST(COALESCE(SUM(", "), 0) AS DOUBLE)"),
            (AggregateFunction::Avg, Dialect::Sqlite) => ("AVG(", ")"),
            (AggregateFunction::Avg, Dialect::Postgres) => ("CAST(AVG(", ") AS DOUBLE PRECISION)"),
            (AggregateFunction::Avg, Dialect::Mysql) => ("CAST(AVG(", ") AS DOUBLE)"),
            (AggregateFunction::Min, _) => ("MIN(", ")"),
            (AggregateFunction::Max, _) => ("MAX(", ")"),
            (AggregateFunction::Count, _) => ("COUNT(", ")"),
        };

        self.push(open);
        self.identifier(field);
        self.push(close);
    }

    /// The key a row is grouped under. A day, week (starting on Monday),
    /// month or year bucket is the `DATE` it starts on; an hour or minute
    /// bucket is the timestamp it starts at.
    fn group_key(&mut self, key: GroupBy) {
        let (field, bucket) = match key {
            GroupBy::Field(field) => return self.identifier(field),
            GroupBy::Date(field, bucket) => (field, bucket),
        };

        if matches!(bucket, DateBucket::Hour | DateBucket::Minute) {
            let hour = bucket == DateBucket::Hour;
            match self.dialect {
                // RFC 3339 text, the shape SQLite stores `DateTime` values in.
                Dialect::Sqlite => {
                    self.push(if hour {
                        "strftime('%Y-%m-%dT%H:00:00+00:00', "
                    } else {
                        "strftime('%Y-%m-%dT%H:%M:00+00:00', "
                    });
                    self.identifier(field);
                    self.sql.push(')');
                }
                Dialect::Postgres => {
                    self.push(if hour { "date_trunc('hour', " } else { "date_trunc('minute', " });
                    self.identifier(field);
                    self.sql.push(')');
                }
                Dialect::Mysql => {
                    self.push("CAST(DATE_FORMAT(");
                    self.identifier(field);
                    self.push(if hour {
                        ", '%Y-%m-%d %H:00:00') AS DATETIME)"
                    } else {
                        ", '%Y-%m-%d %H:%i:00') AS DATETIME)"
                    });
                }
            }
            return;
        }

        match self.dialect {
            // `DateTime` values are RFC 3339 text, which the date functions
            // read (offset included).
            Dialect::Sqlite => {
                self.push("date(");
                self.identifier(field);
                self.push(match bucket {
                    DateBucket::Day | DateBucket::Hour | DateBucket::Minute => ")",
                    // The next Sunday (or the same day), back to its Monday.
                    DateBucket::Week => ", 'weekday 0', '-6 days')",
                    DateBucket::Month => ", 'start of month')",
                    DateBucket::Year => ", 'start of year')",
                });
            }
            Dialect::Postgres => {
                self.push(match bucket {
                    DateBucket::Day | DateBucket::Hour | DateBucket::Minute => "CAST(",
                    DateBucket::Week => "CAST(date_trunc('week', ",
                    DateBucket::Month => "CAST(date_trunc('month', ",
                    DateBucket::Year => "CAST(date_trunc('year', ",
                });
                self.identifier(field);
                self.push(if bucket == DateBucket::Day { " AS DATE)" } else { ") AS DATE)" });
            }
            Dialect::Mysql => match bucket {
                DateBucket::Day | DateBucket::Hour | DateBucket::Minute => {
                    self.push("DATE(");
                    self.identifier(field);
                    self.sql.push(')');
                }
                DateBucket::Week | DateBucket::Month => {
                    self.push("DATE_SUB(DATE(");
                    self.identifier(field);
                    self.push(if bucket == DateBucket::Week {
                        "), INTERVAL WEEKDAY("
                    } else {
                        "), INTERVAL DAYOFMONTH("
                    });
                    self.identifier(field);
                    self.push(if bucket == DateBucket::Week { ") DAY)" } else { ") - 1 DAY)" });
                }
                DateBucket::Year => {
                    self.push("MAKEDATE(YEAR(");
                    self.identifier(field);
                    self.push("), 1)");
                }
            },
        }
    }

    fn order_by(&mut self, order_by: Option<FindOrderBy>, qualifier: Qualifier<'_>) {
        if let Some(order_by) = order_by {
            let (field, direction) = order_by_parts(&order_by);
            self.push(" ORDER BY ");
            self.field(field, qualifier);
            self.push(direction);
        }
    }

    /// The `ORDER BY` of a `ROW_NUMBER()` window: the requested order, or
    /// `<table>.<fallback>`.
    fn partition_order(&mut self, table: &str, order_by: Option<FindOrderBy>, fallback: &str) {
        match order_by {
            Some(order_by) => {
                let (field, direction) = order_by_parts(&order_by);
                self.field(field, Qualifier::Table(table));
                self.push(direction);
            }
            None => self.raw_field(table, fallback),
        }
    }

    /// Opens the outer query that keeps a window of rows per relation key;
    /// [`close_row_window`](Self::close_row_window) closes it.
    fn open_row_window(&mut self) {
        self.push("SELECT * FROM (");
    }

    fn close_row_window(&mut self, skip: i32, limit: i32) {
        self.sql.push(')');
        if self.dialect == Dialect::Mysql {
            self.push(" AS __dinoco_partitioned");
        }

        let skip = skip.max(0);
        if skip > 0 {
            self.push(" WHERE __dinoco_row_num > ");
            self.bind(DinocoValue::Integer(i64::from(skip)));
        }
        if limit >= 0 {
            self.push(if skip > 0 { " AND " } else { " WHERE " });
            self.push("__dinoco_row_num <= ");
            self.bind(DinocoValue::Integer(i64::from(skip) + i64::from(limit)));
        }

        self.push(" ORDER BY __dinoco_relation_key, __dinoco_row_num");
    }

    fn row_number(&mut self, partition: impl FnOnce(&mut Self), order: impl FnOnce(&mut Self)) {
        self.push(", ROW_NUMBER() OVER (PARTITION BY ");
        partition(self);
        self.push(" ORDER BY ");
        order(self);
        self.push(") AS __dinoco_row_num");
    }

    fn conditions(&mut self, conditions: Vec<FindWhere>, qualifier: Qualifier<'_>, separator: &str) {
        for (index, condition) in conditions.into_iter().enumerate() {
            if index > 0 {
                self.push(separator);
            }
            self.condition(condition, qualifier);
        }
    }

    fn condition(&mut self, condition: FindWhere, qualifier: Qualifier<'_>) {
        match condition {
            FindWhere::Eq(field, value) => self.comparison(field, " = ", value, qualifier),
            FindWhere::Neq(field, value) => self.comparison(field, " != ", value, qualifier),
            FindWhere::Gt(field, value) => self.comparison(field, " > ", value, qualifier),
            FindWhere::Gte(field, value) => self.comparison(field, " >= ", value, qualifier),
            FindWhere::Lt(field, value) => self.comparison(field, " < ", value, qualifier),
            FindWhere::Lte(field, value) => self.comparison(field, " <= ", value, qualifier),
            FindWhere::Like(field, value) => self.comparison(field, " LIKE ", value, qualifier),
            FindWhere::FullText(fields, value) => self.full_text(fields, value, qualifier),
            FindWhere::Between(field, start, end) => {
                self.comparison(field, " >= ", start, qualifier);
                self.push(" AND ");
                self.comparison(field, " <= ", end, qualifier);
            }
            FindWhere::Batch(field, values) => {
                if values.is_empty() {
                    self.push("1 = 0");
                    return;
                }

                self.field(field, qualifier);
                self.push(" IN (");
                for (index, value) in values.into_iter().enumerate() {
                    if index > 0 {
                        self.push(", ");
                    }
                    self.bind(value);
                }
                self.sql.push(')');
            }
            FindWhere::Null(field) => {
                self.field(field, qualifier);
                self.push(" IS NULL");
            }
            FindWhere::NotNull(field) => {
                self.field(field, qualifier);
                self.push(" IS NOT NULL");
            }
            FindWhere::ManyToMany(membership) => {
                self.field(membership.local_key, qualifier);
                self.push(if membership.negated { " NOT IN (SELECT " } else { " IN (SELECT " });
                self.identifier(membership.join_local_field);
                self.push(" FROM ");
                self.identifier(membership.join_table);
                self.push(" WHERE ");
                self.condition(*membership.predicate, Qualifier::None);
                self.sql.push(')');
            }
            FindWhere::Relation(relation) => self.relation(relation, qualifier, false),
            FindWhere::And(conditions) => self.group(conditions, qualifier, " AND ", "1 = 1"),
            FindWhere::Or(conditions) => self.group(conditions, qualifier, " OR ", "1 = 0"),
            FindWhere::Not(condition) => {
                self.push("NOT (");
                self.condition(*condition, qualifier);
                self.sql.push(')');
            }
        }
    }

    fn comparison(&mut self, field: &str, operator: &str, value: DinocoValue, qualifier: Qualifier<'_>) {
        self.field(field, qualifier);
        self.push(operator);
        self.bind(value);
    }

    fn group(&mut self, conditions: Vec<FindWhere>, qualifier: Qualifier<'_>, separator: &str, empty: &str) {
        self.sql.push('(');
        if conditions.is_empty() {
            self.push(empty);
        } else {
            self.conditions(conditions, qualifier, separator);
        }
        self.sql.push(')');
    }

    fn full_text(&mut self, fields: &[&str], value: DinocoValue, qualifier: Qualifier<'_>) {
        match self.dialect {
            Dialect::Postgres => {
                self.push("to_tsvector('simple', ");
                for (index, field) in fields.iter().enumerate() {
                    if index > 0 {
                        self.push(" || ' ' || ");
                    }
                    self.push("COALESCE(");
                    self.field(field, qualifier);
                    self.push(", '')");
                }
                self.push(") @@ plainto_tsquery('simple', ");
                self.bind(value);
                self.sql.push(')');
            }
            Dialect::Mysql => {
                self.push("MATCH (");
                self.fields(fields, qualifier);
                self.push(") AGAINST (");
                self.bind(value);
                self.push(" IN NATURAL LANGUAGE MODE)");
            }
            // SQLite has no full-text index here: every field is matched with
            // `LIKE '%value%'`.
            Dialect::Sqlite => {
                let value = match value {
                    DinocoValue::String(value) => DinocoValue::String(format!("%{value}%")),
                    value => value,
                };

                match fields {
                    [] => self.push("1 = 0"),
                    [field] => self.comparison(field, " LIKE ", value, qualifier),
                    [fields @ .., last] => {
                        self.sql.push('(');
                        for field in fields {
                            self.comparison(field, " LIKE ", value.clone(), qualifier);
                            self.push(" OR ");
                        }
                        self.comparison(last, " LIKE ", value, qualifier);
                        self.sql.push(')');
                    }
                }
            }
        }
    }

    /// A relation filter as a correlated `[NOT] EXISTS` subquery.
    /// `materialized` reads the related keys from a derived table instead (see
    /// [`write_where_clause`](Self::write_where_clause)), correlated outside of
    /// it so NULL keys behave the same.
    fn relation(&mut self, relation: RelationMatch, qualifier: Qualifier<'_>, materialized: bool) {
        let RelationMatch { parent_table, parent_field, child_table, child_field, join, quantifier, conditions } =
            relation;
        let depth = qualifier.relation_depth() + 1;

        // Every related row satisfies an empty condition list.
        if quantifier == RelationQuantifier::Every && conditions.is_empty() {
            self.push("1 = 1");
            return;
        }

        if quantifier != RelationQuantifier::Some {
            self.push("NOT ");
        }
        self.push("EXISTS (SELECT 1 FROM ");
        if materialized {
            self.push("(SELECT DISTINCT ");
            self.relation_key(join.map(|join| join.parent_field), child_field, depth);
            self.push(" AS __dinoco_key FROM ");
        }

        // The related table, reached through the join table of a
        // many-to-many relation.
        match join {
            Some(join) => {
                self.identifier(join.table);
                self.push(" AS ");
                self.relation_alias(JOIN_ALIAS_PREFIX, depth);
                self.push(" INNER JOIN ");
                self.identifier(child_table);
                self.push(" AS ");
                self.relation_alias(CHILD_ALIAS_PREFIX, depth);
                self.push(" ON ");
                self.relation_key(None, child_field, depth);
                self.push(" = ");
                self.relation_key(Some(join.child_field), child_field, depth);
            }
            None => {
                self.identifier(child_table);
                self.push(" AS ");
                self.relation_alias(CHILD_ALIAS_PREFIX, depth);
            }
        }

        let outer = |writer: &mut Self| {
            match qualifier {
                Qualifier::None => writer.identifier(parent_table),
                qualifier => writer.qualifier(qualifier),
            }
            writer.sql.push('.');
            writer.identifier(parent_field);
        };

        if materialized {
            if !conditions.is_empty() {
                self.push(" WHERE ");
                self.related_row_filter(quantifier, conditions, depth);
            }
            self.push(") AS ");
            self.relation_alias(KEYS_ALIAS_PREFIX, depth);
            self.push(" WHERE ");
            self.relation_alias(KEYS_ALIAS_PREFIX, depth);
            self.push(".__dinoco_key = ");
            outer(self);
        } else {
            self.push(" WHERE ");
            self.relation_key(join.map(|join| join.parent_field), child_field, depth);
            self.push(" = ");
            outer(self);
            if !conditions.is_empty() {
                self.push(" AND ");
                self.related_row_filter(quantifier, conditions, depth);
            }
        }
        self.sql.push(')');
    }

    /// The column of the relation subquery correlated with the outer row: the
    /// join table's `join_field` for a many-to-many relation, the related
    /// table's `child_field` otherwise.
    fn relation_key(&mut self, join_field: Option<&str>, child_field: &str, depth: usize) {
        match join_field {
            Some(join_field) => {
                self.relation_alias(JOIN_ALIAS_PREFIX, depth);
                self.sql.push('.');
                self.identifier(join_field);
            }
            None => {
                self.relation_alias(CHILD_ALIAS_PREFIX, depth);
                self.sql.push('.');
                self.identifier(child_field);
            }
        }
    }

    /// The condition a related row is checked against. `IS NOT TRUE` also
    /// catches conditions that evaluate to NULL, which a plain `NOT (...)`
    /// would let through as matching.
    fn related_row_filter(&mut self, quantifier: RelationQuantifier, conditions: Vec<FindWhere>, depth: usize) {
        if quantifier == RelationQuantifier::Every {
            self.sql.push('(');
            self.conditions(conditions, Qualifier::Relation(depth), " AND ");
            self.push(") IS NOT TRUE");
        } else {
            self.conditions(conditions, Qualifier::Relation(depth), " AND ");
        }
    }

    fn write_conditions(&mut self, conditions: Vec<FindWhere>, table: &str, separator: &str) {
        for (index, condition) in conditions.into_iter().enumerate() {
            if index > 0 {
                self.push(separator);
            }
            self.write_condition(condition, table);
        }
    }

    fn write_condition(&mut self, condition: FindWhere, table: &str) {
        match condition {
            FindWhere::Relation(relation) if relation_reads_table(&relation, table) => {
                self.relation(relation, Qualifier::None, true);
            }
            FindWhere::And(conditions) => self.write_group(conditions, table, " AND ", "1 = 1"),
            FindWhere::Or(conditions) => self.write_group(conditions, table, " OR ", "1 = 0"),
            FindWhere::Not(condition) => {
                self.push("NOT (");
                self.write_condition(*condition, table);
                self.sql.push(')');
            }
            condition => self.condition(condition, Qualifier::None),
        }
    }

    fn write_group(&mut self, conditions: Vec<FindWhere>, table: &str, separator: &str, empty: &str) {
        self.sql.push('(');
        if conditions.is_empty() {
            self.push(empty);
        } else {
            self.write_conditions(conditions, table, separator);
        }
        self.sql.push(')');
    }

    fn assignment(&mut self, set: UpdateSet) {
        let operator = match set.operation {
            UpdateOperation::Set => None,
            UpdateOperation::Increment => Some(" + "),
            UpdateOperation::Decrement => Some(" - "),
            UpdateOperation::Multiply => Some(" * "),
            UpdateOperation::Divide => Some(" / "),
            UpdateOperation::CurrentTimestamp | UpdateOperation::CurrentDate => {
                let now = self.dialect.current_time();
                self.identifier(set.field);
                self.push(" = ");
                self.push(if set.operation == UpdateOperation::CurrentDate { now.date } else { now.timestamp });
                return;
            }
            UpdateOperation::Connect
            | UpdateOperation::Disconnect
            | UpdateOperation::ConnectManyToMany(_)
            | UpdateOperation::DisconnectManyToMany(_) => return,
        };

        self.identifier(set.field);
        self.push(" = ");
        if let Some(operator) = operator {
            self.identifier(set.field);
            self.push(operator);
        }
        self.bind(set.value);
    }

    /// The parent keys every relation row is matched against, numbered by
    /// ordinal. `with_keys` also selects one bound key per ordinal.
    fn parent_keys(&mut self, key_count: usize, with_keys: bool) {
        match self.dialect {
            Dialect::Mysql => {
                self.push("WITH __dinoco_parent_keys AS (");
                for ordinal in 0..key_count {
                    if ordinal == 0 {
                        self.push("SELECT 0 AS __dinoco_relation_ordinal");
                        if with_keys {
                            self.push(", ");
                            self.placeholder();
                            self.push(" AS __dinoco_relation_key");
                        }
                    } else {
                        self.push(" UNION ALL SELECT ");
                        self.number(ordinal);
                        if with_keys {
                            self.push(", ");
                            self.placeholder();
                        }
                    }
                }
            }
            Dialect::Sqlite | Dialect::Postgres => {
                self.push(if with_keys {
                    "WITH __dinoco_parent_keys(__dinoco_relation_ordinal, __dinoco_relation_key) AS (VALUES "
                } else {
                    "WITH __dinoco_parent_keys(__dinoco_relation_ordinal) AS (VALUES "
                });
                for ordinal in 0..key_count {
                    if ordinal > 0 {
                        self.push(", ");
                    }
                    self.sql.push('(');
                    self.number(ordinal);
                    if with_keys {
                        self.push(", ");
                        self.placeholder();
                        if self.dialect == Dialect::Postgres {
                            self.postgres_key_cast(self.placeholders - 1);
                        }
                    }
                    self.sql.push(')');
                }
            }
        }
        self.sql.push(')');
    }

    /// PostgreSQL types a parameter it can't infer from its context as
    /// `text`, and nothing in a `VALUES` list gives a parent key a context:
    /// comparing it with a `bigint` column would then fail with `operator
    /// does not exist: text = bigint`. Casts the key bound at `index` to the
    /// type of the value it holds. A `NULL` key keeps no cast and takes the
    /// type of the other keys; without bound keys nothing is written.
    fn postgres_key_cast(&mut self, index: usize) {
        let cast = match self.params.get(index) {
            Some(DinocoValue::Integer(_)) => "::bigint",
            Some(DinocoValue::Float(_)) => "::double precision",
            Some(DinocoValue::String(_)) => "::text",
            Some(DinocoValue::Boolean(_)) => "::boolean",
            Some(DinocoValue::Bytes(_)) => "::bytea",
            Some(DinocoValue::Json(_)) => "::jsonb",
            Some(DinocoValue::DateTime(_)) => "::timestamp",
            Some(DinocoValue::Date(_)) => "::date",
            Some(DinocoValue::Enum(name, _)) => {
                // The native enum type, created under its quoted name.
                self.sql.push_str("::");
                write_quoted(&mut self.sql, '"', name);
                return;
            }
            Some(DinocoValue::Null) | None => return,
        };
        self.sql.push_str(cast);
    }
}

fn order_by_parts(order_by: &FindOrderBy) -> (&'static str, &'static str) {
    match *order_by {
        FindOrderBy::Asc(field) => (field, " ASC"),
        FindOrderBy::Desc(field) => (field, " DESC"),
    }
}

fn write_number(sql: &mut String, mut number: usize) {
    let mut digits = [0u8; 20];
    let mut start = digits.len();
    loop {
        start -= 1;
        digits[start] = b'0' + (number % 10) as u8;
        number /= 10;
        if number == 0 {
            break;
        }
    }
    // The buffer only ever holds ASCII digits.
    sql.push_str(std::str::from_utf8(&digits[start..]).unwrap_or_default());
}

fn write_identifier(sql: &mut String, dialect: Dialect, identifier: &str) {
    if identifier == "*" || !(identifier_requires_quotes(identifier) || is_reserved_identifier(identifier)) {
        sql.push_str(identifier);
    } else {
        write_quoted(sql, dialect.quote(), identifier);
    }
}

fn write_quoted(sql: &mut String, quote: char, identifier: &str) {
    sql.push(quote);
    if identifier.contains(quote) {
        for ch in identifier.chars() {
            if ch == quote {
                sql.push(quote);
            }
            sql.push(ch);
        }
    } else {
        sql.push_str(identifier);
    }
    sql.push(quote);
}

fn identifier_requires_quotes(identifier: &str) -> bool {
    let mut bytes = identifier.bytes();
    !matches!(bytes.next(), Some(byte) if byte.is_ascii_lowercase() || byte == b'_')
        || bytes.any(|byte| !(byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_'))
}

/// Only called for identifiers that are already lowercase: any other one is
/// quoted by [`identifier_requires_quotes`].
fn is_reserved_identifier(identifier: &str) -> bool {
    RESERVED_IDENTIFIERS.binary_search(&identifier).is_ok()
}

/// Upper bound of the values `conditions` bind, used to size buffers.
fn bound_values(conditions: &[FindWhere]) -> usize {
    conditions.iter().map(condition_bound_values).sum()
}

fn condition_bound_values(condition: &FindWhere) -> usize {
    match condition {
        FindWhere::Eq(..)
        | FindWhere::Neq(..)
        | FindWhere::Gt(..)
        | FindWhere::Gte(..)
        | FindWhere::Lt(..)
        | FindWhere::Lte(..)
        | FindWhere::Like(..) => 1,
        FindWhere::FullText(fields, _) => fields.len().max(1),
        FindWhere::Between(..) => 2,
        FindWhere::Batch(_, values) => values.len(),
        FindWhere::Null(_) | FindWhere::NotNull(_) => 0,
        FindWhere::ManyToMany(membership) => condition_bound_values(&membership.predicate),
        FindWhere::Relation(relation) => bound_values(&relation.conditions),
        FindWhere::And(conditions) | FindWhere::Or(conditions) => bound_values(conditions),
        FindWhere::Not(condition) => condition_bound_values(condition),
    }
}

/// A rough size of the SQL for `fields` and `values` bound values, enough to
/// avoid most buffer growth.
fn sql_capacity(fields: usize, values: usize) -> usize {
    96 + fields * 16 + values * 24
}

fn writer(dialect: Dialect, fields: usize, conditions: &[FindWhere], extra_values: usize) -> SqlWriter {
    let values = bound_values(conditions) + extra_values;
    SqlWriter::new(dialect, sql_capacity(fields, values), Vec::with_capacity(values))
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
        FindWhere::ManyToMany(membership) => membership.join_table == table,
        FindWhere::And(conditions) | FindWhere::Or(conditions) => {
            conditions.iter().any(|condition| condition_reads_table(condition, table))
        }
        FindWhere::Not(condition) => condition_reads_table(condition, table),
        _ => false,
    }
}

pub(crate) fn find(dialect: Dialect, query: FindQuery) -> (String, Vec<DinocoValue>) {
    let FindQuery { fields, from, conditions, limit, skip, order_by } = query;
    let mut sql = writer(dialect, fields.len(), &conditions, 2);

    sql.push("SELECT ");
    sql.identifiers(fields);
    sql.push(" FROM ");
    sql.identifier(from);
    sql.find_tail(conditions, order_by, limit, skip, Qualifier::None);

    sql.finish()
}

pub(crate) fn insert(dialect: Dialect, query: InsertQuery) -> (String, Vec<DinocoValue>) {
    let InsertQuery { table, fields, rows, returning, on_conflict_do_nothing } = query;
    let values = rows.iter().map(Vec::len).sum::<usize>();
    let placeholder_width = if dialect == Dialect::Postgres { 7 } else { 3 };
    let capacity = 64
        + table.len()
        + fields.len() * 16
        + rows.len() * (fields.len() * placeholder_width + 4)
        + returning.map_or(0, |returning| 12 + returning.len() * 16)
        + on_conflict_do_nothing.map_or(0, |target| 32 + target.len() * 16);
    let mut sql = SqlWriter::new(dialect, capacity, Vec::with_capacity(values));

    sql.push("INSERT INTO ");
    sql.identifier(table);
    sql.push(" (");
    sql.identifiers(&fields);
    sql.push(") VALUES ");
    for (index, row) in rows.into_iter().enumerate() {
        if index > 0 {
            sql.push(", ");
        }
        sql.sql.push('(');
        sql.placeholders(fields.len());
        sql.sql.push(')');
        sql.params.extend(row);
    }
    if let Some(target) = on_conflict_do_nothing {
        if dialect == Dialect::Mysql {
            // MySQL takes no conflict target and has no `DO NOTHING`: assigning
            // a column to itself leaves the existing row as it is.
            let column = target.first().or(fields.first()).copied().unwrap_or("id");
            sql.push(" ON DUPLICATE KEY UPDATE ");
            sql.identifier(column);
            sql.push(" = ");
            sql.identifier(column);
        } else {
            sql.push(" ON CONFLICT (");
            sql.identifiers(target);
            sql.push(") DO NOTHING");
        }
    }
    sql.returning(returning);

    sql.finish()
}

pub(crate) fn lookup(dialect: Dialect, query: LookupQuery) -> (String, Vec<DinocoValue>) {
    let LookupQuery { table, fields, key_fields, keys } = query;
    let values = keys.iter().map(Vec::len).sum::<usize>();
    // `(0, $1::bigint), ` per tuple on PostgreSQL, `(0, ?), ` elsewhere.
    let per_value = if dialect == Dialect::Postgres { 20 } else { 8 };
    let capacity = sql_capacity(fields.len() + key_fields.len() * 2, 0) + keys.len() * 8 + values * per_value;
    let mut sql = SqlWriter::new(dialect, capacity, Vec::with_capacity(values));

    sql.push("WITH __dinoco_lookup_keys");
    match dialect {
        Dialect::Mysql => {
            sql.push(" AS (");
            for (ordinal, key) in keys.into_iter().enumerate() {
                if ordinal == 0 {
                    sql.push("SELECT 0 AS __dinoco_lookup_ordinal");
                } else {
                    sql.push(" UNION ALL SELECT ");
                    sql.number(ordinal);
                }
                for (index, value) in key.into_iter().enumerate() {
                    sql.push(", ");
                    sql.bind(value);
                    if ordinal == 0 {
                        sql.push(" AS __dinoco_lookup_key_");
                        sql.number(index);
                    }
                }
            }
        }
        Dialect::Sqlite | Dialect::Postgres => {
            sql.push("(__dinoco_lookup_ordinal");
            for index in 0..key_fields.len() {
                sql.push(", __dinoco_lookup_key_");
                sql.number(index);
            }
            sql.push(") AS (VALUES ");
            for (ordinal, key) in keys.into_iter().enumerate() {
                if ordinal > 0 {
                    sql.push(", ");
                }
                sql.sql.push('(');
                sql.number(ordinal);
                for value in key {
                    sql.push(", ");
                    sql.bind(value);
                    if dialect == Dialect::Postgres {
                        sql.postgres_key_cast(sql.placeholders - 1);
                    }
                }
                sql.sql.push(')');
            }
        }
    }
    sql.push(") SELECT ");
    sql.fields(fields, Qualifier::Table(table));
    sql.push(" FROM __dinoco_lookup_keys INNER JOIN ");
    sql.identifier(table);
    sql.push(" ON ");
    for (index, field) in key_fields.iter().enumerate() {
        if index > 0 {
            sql.push(" AND ");
        }
        sql.raw_field(table, field);
        sql.push(" = __dinoco_lookup_keys.__dinoco_lookup_key_");
        sql.number(index);
    }
    sql.push(" ORDER BY __dinoco_lookup_keys.__dinoco_lookup_ordinal");

    sql.finish()
}

pub(crate) fn aggregate(dialect: Dialect, query: AggregateQuery) -> (String, Vec<DinocoValue>) {
    let AggregateQuery { source: FindQuery { from, conditions, limit, skip, order_by, .. }, aggregate, group } = query;
    let windowed = limit >= 0 || skip >= 0;
    let mut sql = writer(dialect, 6, &conditions, 4);

    sql.push("SELECT ");
    if let Some(group) = group {
        sql.group_key(group.key);
        sql.push(" AS __dinoco_group_key, ");
    }
    sql.aggregate(aggregate);
    if group.is_some() {
        sql.push(" AS __dinoco_aggregate");
    }

    sql.push(" FROM ");
    if windowed {
        // The window of rows aggregated, with only the columns read outside.
        sql.push("(SELECT ");
        sql.identifier(aggregate.field);
        if let Some(key) = group.map(|group| group.key.field()).filter(|key| *key != aggregate.field) {
            sql.push(", ");
            sql.identifier(key);
        }
        sql.push(" FROM ");
        sql.identifier(from);
        sql.find_tail(conditions, order_by, limit, skip, Qualifier::None);
        sql.push(") AS __dinoco_rows");
    } else {
        sql.identifier(from);
        sql.where_clause(conditions, Qualifier::None);
    }

    if let Some(AggregateGroup { order_by, limit, skip, .. }) = group {
        sql.push(" GROUP BY __dinoco_group_key ORDER BY ");
        sql.push(match order_by.unwrap_or(GroupOrder::KeyAsc) {
            GroupOrder::KeyAsc => "__dinoco_group_key ASC",
            GroupOrder::KeyDesc => "__dinoco_group_key DESC",
            GroupOrder::AggregateAsc => "__dinoco_aggregate ASC, __dinoco_group_key ASC",
            GroupOrder::AggregateDesc => "__dinoco_aggregate DESC, __dinoco_group_key ASC",
        });
        sql.page(limit, skip);
    }

    sql.finish()
}

pub(crate) fn update(dialect: Dialect, query: UpdateQuery) -> (String, Vec<DinocoValue>) {
    let UpdateQuery { table, sets, conditions, returning } = query;
    let mut sql = writer(dialect, sets.len() + returning.map_or(0, <[_]>::len), &conditions, sets.len());

    sql.push("UPDATE ");
    sql.identifier(table);
    sql.push(" SET ");
    let mut first = true;
    for set in sets.into_iter().filter(|set| set.operation.is_scalar()) {
        if !first {
            sql.push(", ");
        }
        first = false;
        sql.assignment(set);
    }
    sql.write_where_clause(conditions, table);
    sql.returning(returning);

    sql.finish()
}

pub(crate) fn delete(dialect: Dialect, query: DeleteQuery) -> (String, Vec<DinocoValue>) {
    let DeleteQuery { table, conditions, returning } = query;
    let mut sql = writer(dialect, returning.map_or(0, <[_]>::len), &conditions, 0);

    sql.push("DELETE FROM ");
    sql.identifier(table);
    sql.write_where_clause(conditions, table);
    sql.returning(returning);

    sql.finish()
}

pub(crate) fn count(dialect: Dialect, query: CountQuery) -> (String, Vec<DinocoValue>) {
    let CountQuery { table, conditions } = query;
    let mut sql = writer(dialect, 0, &conditions, 0);

    sql.push("SELECT COUNT(*) FROM ");
    sql.identifier(table);
    sql.where_clause(conditions, Qualifier::None);

    sql.finish()
}

pub(crate) fn exists(dialect: Dialect, query: ExistsQuery) -> (String, Vec<DinocoValue>) {
    let ExistsQuery { table, conditions } = query;
    let mut sql = writer(dialect, 0, &conditions, 0);

    sql.push("SELECT EXISTS(SELECT 1 FROM ");
    sql.identifier(table);
    sql.where_clause(conditions, Qualifier::None);
    sql.sql.push(')');

    sql.finish()
}

pub(crate) fn relation_count(dialect: Dialect, query: RelationCountQuery) -> (String, Vec<DinocoValue>) {
    let RelationCountQuery {
        parent_table,
        child_table,
        parent_field,
        child_field,
        parent_conditions,
        child_conditions,
    } = query;
    let mut sql = writer(dialect, 4, &parent_conditions, bound_values(&child_conditions));

    sql.push("SELECT COUNT(*) FROM ");
    sql.identifier(child_table);
    sql.push(" WHERE ");
    sql.identifier(child_field);
    sql.push(" IN (SELECT ");
    sql.identifier(parent_field);
    sql.push(" FROM ");
    sql.identifier(parent_table);
    sql.where_clause(parent_conditions, Qualifier::Table(parent_table));
    sql.sql.push(')');
    sql.and_clause(child_conditions, Qualifier::Table(child_table));

    sql.finish()
}

pub(crate) fn relation_batch(dialect: Dialect, query: RelationBatchQuery) -> (String, Vec<DinocoValue>) {
    let RelationBatchQuery {
        query: FindQuery { fields, from: table, conditions, limit, skip, order_by },
        relation_key_field,
    } = query;
    let partitioned = limit >= 0 || skip >= 0;
    let mut sql = writer(dialect, fields.len() + 4, &conditions, 2);

    if partitioned {
        sql.open_row_window();
    }
    sql.push("SELECT ");
    sql.fields(fields, Qualifier::Table(table));
    sql.push(", ");
    sql.field(relation_key_field, Qualifier::Table(table));
    sql.push(" AS __dinoco_relation_key");

    if partitioned {
        sql.row_number(
            |sql| sql.raw_field(table, relation_key_field),
            |sql| sql.partition_order(table, order_by, relation_key_field),
        );
        sql.push(" FROM ");
        sql.identifier(table);
        sql.where_clause(conditions, Qualifier::Table(table));
        sql.close_row_window(skip, limit);
    } else {
        sql.push(" FROM ");
        sql.identifier(table);
        sql.find_tail(conditions, order_by, limit, skip, Qualifier::Table(table));
    }

    sql.finish()
}

pub(crate) fn relation_join(dialect: Dialect, query: RelationJoinQuery) -> (String, Vec<DinocoValue>) {
    let RelationJoinQuery {
        query: FindQuery { fields, conditions, limit, skip, order_by, .. },
        parent_table,
        child_table,
        parent_field,
        child_field,
        key_count,
    } = query;
    let partitioned = limit >= 0 || skip >= 0;
    let self_relation = parent_table == child_table;
    let (parent, child) =
        if self_relation { ("__dinoco_parent", "__dinoco_child") } else { (parent_table, child_table) };
    let mut sql = writer(dialect, fields.len() + 4 + key_count, &conditions, 2);

    if partitioned {
        sql.open_row_window();
    }
    sql.push("SELECT ");
    for field in fields {
        sql.field(field, Qualifier::Table(child));
        sql.push(", ");
    }
    sql.field(child_field, Qualifier::Table(child));
    sql.push(", ");
    sql.field(parent_field, Qualifier::Table(parent));
    sql.push(" AS __dinoco_relation_key");
    if partitioned {
        sql.row_number(
            |sql| sql.raw_field(parent, parent_field),
            |sql| sql.partition_order(child, order_by.clone(), child_field),
        );
    }

    sql.push(" FROM ");
    sql.identifier(parent_table);
    if self_relation {
        sql.push(" AS __dinoco_parent");
    }
    sql.push(" LEFT JOIN ");
    sql.identifier(child_table);
    if self_relation {
        sql.push(" AS __dinoco_child");
    }
    sql.push(" ON ");
    sql.raw_field(parent, parent_field);
    sql.push(" = ");
    sql.raw_field(child, child_field);
    sql.push(" WHERE ");
    sql.raw_field(parent, parent_field);
    sql.push(" IN (");
    sql.placeholders(key_count);
    sql.sql.push(')');
    sql.and_clause(conditions, Qualifier::Table(child));

    if partitioned {
        sql.close_row_window(skip, limit);
    } else {
        sql.order_by(order_by, Qualifier::Table(child));
    }

    sql.finish()
}

pub(crate) fn relation_occurrence(dialect: Dialect, query: RelationOccurrenceQuery) -> (String, Vec<DinocoValue>) {
    relation_occurrence_with_keys(dialect, query, Vec::new())
}

/// [`relation_occurrence`] with the parent keys bound in front of the query's
/// own values.
pub(crate) fn relation_occurrence_with_keys(
    dialect: Dialect,
    query: RelationOccurrenceQuery,
    keys: Vec<DinocoValue>,
) -> (String, Vec<DinocoValue>) {
    let RelationOccurrenceQuery {
        query: FindQuery { fields, from, conditions, limit, skip, order_by },
        child_field,
        key_count,
    } = query;
    let partitioned = limit >= 0 || skip >= 0;
    let values = keys.len() + bound_values(&conditions) + 2;
    let mut params = keys;
    params.reserve(values - params.len());
    // `(0, $1::bigint), ` per bound key on PostgreSQL, `(0, ?), ` elsewhere.
    let per_key = if dialect == Dialect::Postgres && !params.is_empty() { 20 } else { 12 };
    let mut sql = SqlWriter::new(dialect, sql_capacity(fields.len() + 6, values) + key_count * per_key, params);

    if partitioned {
        sql.open_row_window();
    }
    sql.parent_keys(key_count, true);
    sql.push(" SELECT ");
    for field in fields {
        sql.field(field, Qualifier::Table(from));
        sql.push(", ");
    }
    sql.push("__dinoco_parent_keys.__dinoco_relation_key, ");
    sql.push(PARENT_KEYS_ORDINAL);
    if partitioned {
        sql.row_number(
            |sql| sql.push(PARENT_KEYS_ORDINAL),
            |sql| sql.partition_order(from, order_by.clone(), child_field),
        );
    }
    sql.push(" FROM __dinoco_parent_keys INNER JOIN ");
    sql.identifier(from);
    sql.push(" ON __dinoco_parent_keys.__dinoco_relation_key = ");
    sql.raw_field(from, child_field);
    sql.and_clause(conditions, Qualifier::Table(from));

    if partitioned {
        sql.close_row_window(skip, limit);
    } else {
        sql.order_by(order_by, Qualifier::Table(from));
    }

    sql.finish()
}

pub(crate) fn many_to_many_relation(dialect: Dialect, query: ManyToManyRelationQuery) -> (String, Vec<DinocoValue>) {
    many_to_many_relation_with_keys(dialect, query, Vec::new())
}

/// [`many_to_many_relation`] with the parent keys bound in front of the
/// query's own values.
pub(crate) fn many_to_many_relation_with_keys(
    dialect: Dialect,
    query: ManyToManyRelationQuery,
    keys: Vec<DinocoValue>,
) -> (String, Vec<DinocoValue>) {
    let ManyToManyRelationQuery {
        query: FindQuery { fields, from, conditions, limit, skip, order_by },
        join_table,
        child_field,
        join_parent_field,
        join_child_field,
        key_count,
        ..
    } = query;
    let partitioned = limit >= 0 || skip >= 0;
    let values = keys.len() + bound_values(&conditions) + 2;
    let mut params = keys;
    params.reserve(values - params.len());
    let capacity =
        sql_capacity(fields.len() + 6, values) + key_count * (64 + join_table.len() + join_parent_field.len());
    let mut sql = SqlWriter::new(dialect, capacity, params);

    if partitioned {
        sql.open_row_window();
    }
    sql.parent_keys(key_count, false);
    sql.push(" SELECT ");
    for field in fields {
        sql.field(field, Qualifier::Table(from));
        sql.push(", ");
    }
    sql.raw_field(join_table, join_parent_field);
    sql.push(" AS __dinoco_relation_key, ");
    sql.push(PARENT_KEYS_ORDINAL);
    if partitioned {
        sql.row_number(
            |sql| sql.push(PARENT_KEYS_ORDINAL),
            |sql| sql.partition_order(from, order_by.clone(), child_field),
        );
    }

    sql.push(" FROM __dinoco_parent_keys INNER JOIN ");
    sql.identifier(join_table);
    sql.push(" ON ");
    for ordinal in 0..key_count {
        if ordinal > 0 {
            sql.push(" OR ");
        }
        sql.push("(__dinoco_parent_keys.__dinoco_relation_ordinal = ");
        sql.number(ordinal);
        sql.push(" AND ");
        sql.raw_field(join_table, join_parent_field);
        sql.push(" = ");
        sql.placeholder();
        sql.sql.push(')');
    }
    sql.push(" INNER JOIN ");
    sql.identifier(from);
    sql.push(" ON ");
    sql.raw_field(join_table, join_child_field);
    sql.push(" = ");
    sql.raw_field(from, child_field);
    sql.and_clause(conditions, Qualifier::Table(from));

    if partitioned {
        sql.close_row_window(skip, limit);
    } else {
        sql.order_by(order_by, Qualifier::Table(from));
    }

    sql.finish()
}

pub(crate) fn many_to_many_relation_count(
    dialect: Dialect,
    query: ManyToManyRelationCountQuery,
) -> (String, Vec<DinocoValue>) {
    let ManyToManyRelationCountQuery {
        parent_table,
        child_table,
        join_table,
        parent_field,
        child_field,
        join_parent_field,
        join_child_field,
        parent_conditions,
        child_conditions,
    } = query;
    let mut sql = writer(dialect, 8, &parent_conditions, bound_values(&child_conditions));

    sql.push("SELECT COUNT(*) FROM ");
    sql.identifier(join_table);
    sql.push(" INNER JOIN ");
    sql.identifier(child_table);
    sql.push(" ON ");
    sql.raw_field(join_table, join_child_field);
    sql.push(" = ");
    sql.raw_field(child_table, child_field);
    sql.push(" WHERE ");
    sql.raw_field(join_table, join_parent_field);
    sql.push(" IN (SELECT ");
    sql.identifier(parent_field);
    sql.push(" FROM ");
    sql.identifier(parent_table);
    sql.where_clause(parent_conditions, Qualifier::Table(parent_table));
    sql.sql.push(')');
    sql.and_clause(child_conditions, Qualifier::Table(child_table));

    sql.finish()
}

/// `value` as the body of a single-quoted SQL string literal.
pub(crate) fn escape_literal(value: &str) -> String {
    value.replace('\'', "''")
}

pub(crate) fn referential_action(action: crate::ReferentialAction) -> &'static str {
    match action {
        crate::ReferentialAction::Cascade => "CASCADE",
        crate::ReferentialAction::Restrict => "RESTRICT",
        crate::ReferentialAction::NoAction => "NO ACTION",
        crate::ReferentialAction::SetNull => "SET NULL",
        crate::ReferentialAction::SetDefault => "SET DEFAULT",
    }
}

/// The query half of a [`crate::DinocoSqlCompiler`] implementation for
/// `$dialect`; the migration half stays with each backend.
macro_rules! query_compiler_methods {
    ($dialect:expr) => {
        fn compile_find_query(&self, query: $crate::FindQuery) -> (String, Vec<$crate::DinocoValue>) {
            $crate::backends::sql::find($dialect, query)
        }

        fn compile_insert_query(&self, query: $crate::InsertQuery) -> (String, Vec<$crate::DinocoValue>) {
            $crate::backends::sql::insert($dialect, query)
        }

        fn compile_update_query(&self, query: $crate::UpdateQuery) -> (String, Vec<$crate::DinocoValue>) {
            $crate::backends::sql::update($dialect, query)
        }

        fn compile_delete_query(&self, query: $crate::DeleteQuery) -> (String, Vec<$crate::DinocoValue>) {
            $crate::backends::sql::delete($dialect, query)
        }

        fn compile_count_query(&self, query: $crate::CountQuery) -> (String, Vec<$crate::DinocoValue>) {
            $crate::backends::sql::count($dialect, query)
        }

        fn compile_exists_query(&self, query: $crate::ExistsQuery) -> (String, Vec<$crate::DinocoValue>) {
            $crate::backends::sql::exists($dialect, query)
        }

        fn compile_aggregate_query(&self, query: $crate::AggregateQuery) -> (String, Vec<$crate::DinocoValue>) {
            $crate::backends::sql::aggregate($dialect, query)
        }

        fn compile_lookup_query(&self, query: $crate::LookupQuery) -> (String, Vec<$crate::DinocoValue>) {
            $crate::backends::sql::lookup($dialect, query)
        }

        fn compile_relation_count_query(
            &self,
            query: $crate::RelationCountQuery,
        ) -> (String, Vec<$crate::DinocoValue>) {
            $crate::backends::sql::relation_count($dialect, query)
        }

        fn compile_relation_batch_query(
            &self,
            query: $crate::RelationBatchQuery,
        ) -> (String, Vec<$crate::DinocoValue>) {
            $crate::backends::sql::relation_batch($dialect, query)
        }

        fn compile_relation_join_query(&self, query: $crate::RelationJoinQuery) -> (String, Vec<$crate::DinocoValue>) {
            $crate::backends::sql::relation_join($dialect, query)
        }

        fn compile_relation_occurrence_query(
            &self,
            query: $crate::RelationOccurrenceQuery,
        ) -> (String, Vec<$crate::DinocoValue>) {
            $crate::backends::sql::relation_occurrence($dialect, query)
        }

        fn compile_relation_occurrence_query_with_keys(
            &self,
            query: $crate::RelationOccurrenceQuery,
            keys: Vec<$crate::DinocoValue>,
        ) -> (String, Vec<$crate::DinocoValue>) {
            $crate::backends::sql::relation_occurrence_with_keys($dialect, query, keys)
        }

        fn compile_many_to_many_relation_query(
            &self,
            query: $crate::ManyToManyRelationQuery,
        ) -> (String, Vec<$crate::DinocoValue>) {
            $crate::backends::sql::many_to_many_relation($dialect, query)
        }

        fn compile_many_to_many_relation_count_query(
            &self,
            query: $crate::ManyToManyRelationCountQuery,
        ) -> (String, Vec<$crate::DinocoValue>) {
            $crate::backends::sql::many_to_many_relation_count($dialect, query)
        }
    };
}

pub(crate) use query_compiler_methods;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reserved_identifiers_are_sorted_lowercase_and_unique() {
        assert!(RESERVED_IDENTIFIERS.windows(2).all(|pair| pair[0] < pair[1]));
        assert!(RESERVED_IDENTIFIERS.iter().all(|word| !word.bytes().any(|byte| byte.is_ascii_uppercase())));
    }

    #[test]
    fn reserved_words_of_every_dialect_are_quoted() {
        for (dialect, quote) in [(Dialect::Postgres, '"'), (Dialect::Mysql, '`'), (Dialect::Sqlite, '"')] {
            for word in ["user", "order", "key", "range", "end", "desc", "values", "to", "action", "rank"] {
                assert_eq!(dialect.identifier(word), format!("{quote}{word}{quote}"));
            }
            assert_eq!(dialect.identifier("users"), "users");
            assert_eq!(dialect.identifier("created_at"), "created_at");
            assert_eq!(dialect.identifier("User"), format!("{quote}User{quote}"));
        }
    }
}
