use crate::{
    CountQuery, DeleteQuery, DinocoSqlCompiler, DinocoValue, FindOrderBy, FindQuery, FindWhere, InsertQuery,
    RelationBatchQuery, RelationCountQuery, RelationJoinQuery, UpdateQuery,
};

use super::{PgBouncerAdapter, PostgresAdapter};

impl DinocoSqlCompiler for PostgresAdapter {
    fn compile_find_query(&self, query: FindQuery) -> (String, Vec<DinocoValue>) {
        compile_find_query(query)
    }

    fn compile_insert_query(&self, query: InsertQuery) -> (String, Vec<DinocoValue>) {
        compile_insert_query(query)
    }

    fn compile_update_query(&self, query: UpdateQuery) -> (String, Vec<DinocoValue>) {
        compile_update_query(query)
    }

    fn compile_delete_query(&self, query: DeleteQuery) -> (String, Vec<DinocoValue>) {
        compile_delete_query(query)
    }

    fn compile_count_query(&self, query: CountQuery) -> (String, Vec<DinocoValue>) {
        compile_count_query(query)
    }

    fn compile_relation_count_query(&self, query: RelationCountQuery) -> (String, Vec<DinocoValue>) {
        compile_relation_count_query(query)
    }

    fn compile_relation_batch_query(&self, query: RelationBatchQuery) -> (String, Vec<DinocoValue>) {
        compile_relation_batch_query(query)
    }

    fn compile_relation_join_query(&self, query: RelationJoinQuery) -> (String, Vec<DinocoValue>) {
        compile_relation_join_query(query)
    }
}

impl DinocoSqlCompiler for PgBouncerAdapter {
    fn compile_find_query(&self, query: FindQuery) -> (String, Vec<DinocoValue>) {
        compile_find_query(query)
    }

    fn compile_insert_query(&self, query: InsertQuery) -> (String, Vec<DinocoValue>) {
        compile_insert_query(query)
    }

    fn compile_update_query(&self, query: UpdateQuery) -> (String, Vec<DinocoValue>) {
        compile_update_query(query)
    }

    fn compile_delete_query(&self, query: DeleteQuery) -> (String, Vec<DinocoValue>) {
        compile_delete_query(query)
    }

    fn compile_count_query(&self, query: CountQuery) -> (String, Vec<DinocoValue>) {
        compile_count_query(query)
    }

    fn compile_relation_count_query(&self, query: RelationCountQuery) -> (String, Vec<DinocoValue>) {
        compile_relation_count_query(query)
    }

    fn compile_relation_batch_query(&self, query: RelationBatchQuery) -> (String, Vec<DinocoValue>) {
        compile_relation_batch_query(query)
    }

    fn compile_relation_join_query(&self, query: RelationJoinQuery) -> (String, Vec<DinocoValue>) {
        compile_relation_join_query(query)
    }
}

fn compile_find_query(query: FindQuery) -> (String, Vec<DinocoValue>) {
    let mut sql = format!("SELECT {} FROM {}", query.fields.join(", "), query.from);
    let mut placeholders = Placeholder::default();
    let params =
        append_find_tail(&mut sql, query.conditions, query.order_by, query.limit, query.skip, None, &mut placeholders);

    (sql, params)
}

fn compile_insert_query(query: InsertQuery) -> (String, Vec<DinocoValue>) {
    let mut placeholders = Placeholder::default();
    let placeholders_sql = (0..query.rows.len())
        .map(|_| format!("({})", (0..query.fields.len()).map(|_| placeholders.next()).collect::<Vec<_>>().join(", ")))
        .collect::<Vec<_>>()
        .join(", ");
    let params = query.rows.into_iter().flatten().collect::<Vec<_>>();
    let mut sql = format!("INSERT INTO {} ({}) VALUES {placeholders_sql}", query.table, query.fields.join(", "));

    if let Some(returning) = query.returning {
        sql.push_str(" RETURNING ");
        sql.push_str(&returning.join(", "));
    }

    (sql, params)
}

fn compile_update_query(query: UpdateQuery) -> (String, Vec<DinocoValue>) {
    let mut placeholders = Placeholder::default();
    let sets = query.sets.iter().filter(|set| set.operation == crate::UpdateOperation::Set).collect::<Vec<_>>();
    let mut params = sets.iter().map(|set| set.value.clone()).collect::<Vec<_>>();
    let set_sql =
        sets.iter().map(|set| format!("{} = {}", set.field, placeholders.next())).collect::<Vec<_>>().join(", ");
    let mut sql = format!("UPDATE {} SET {set_sql}", query.table);

    params.extend(append_conditions(&mut sql, query.conditions, None, &mut placeholders));

    if let Some(returning) = query.returning {
        sql.push_str(" RETURNING ");
        sql.push_str(&returning.join(", "));
    }

    (sql, params)
}

fn compile_delete_query(query: DeleteQuery) -> (String, Vec<DinocoValue>) {
    let mut sql = format!("DELETE FROM {}", query.table);
    let mut placeholders = Placeholder::default();
    let params = append_conditions(&mut sql, query.conditions, None, &mut placeholders);

    if let Some(returning) = query.returning {
        sql.push_str(" RETURNING ");
        sql.push_str(&returning.join(", "));
    }

    (sql, params)
}

fn compile_count_query(query: CountQuery) -> (String, Vec<DinocoValue>) {
    let mut sql = format!("SELECT COUNT(*) FROM {}", query.table);
    let mut placeholders = Placeholder::default();
    let params = append_conditions(&mut sql, query.conditions, None, &mut placeholders);

    (sql, params)
}

fn compile_relation_count_query(query: RelationCountQuery) -> (String, Vec<DinocoValue>) {
    let mut parent_sql = format!("SELECT {} FROM {}", query.parent_field, query.parent_table);
    let mut placeholders = Placeholder::default();
    let mut params =
        append_conditions(&mut parent_sql, query.parent_conditions, Some(query.parent_table), &mut placeholders);
    let mut sql = format!("SELECT COUNT(*) FROM {} WHERE {} IN ({parent_sql})", query.child_table, query.child_field);
    append_and_conditions(&mut sql, &mut params, query.child_conditions, Some(query.child_table), &mut placeholders);

    (sql, params)
}

fn compile_relation_batch_query(query: RelationBatchQuery) -> (String, Vec<DinocoValue>) {
    let fields = query.query.fields.iter().map(|field| format!("{}.{}", query.query.from, field)).collect::<Vec<_>>();
    let mut select_fields = fields.join(", ");
    select_fields.push_str(", ");
    select_fields.push_str(query.query.from);
    select_fields.push('.');
    select_fields.push_str(query.relation_key_field);
    select_fields.push_str(" AS __dinoco_relation_key");

    if query.query.limit >= 0 || query.query.skip >= 0 {
        return compile_partitioned_relation_batch_query(query, select_fields);
    }

    let table = query.query.from;
    let mut sql = format!("SELECT {select_fields} FROM {table}");
    let mut placeholders = Placeholder::default();
    let params = append_find_tail(
        &mut sql,
        query.query.conditions,
        query.query.order_by,
        query.query.limit,
        query.query.skip,
        Some(table),
        &mut placeholders,
    );

    (sql, params)
}

fn compile_relation_join_query(query: RelationJoinQuery) -> (String, Vec<DinocoValue>) {
    if query.query.limit >= 0 || query.query.skip >= 0 {
        return compile_partitioned_relation_join_query(query);
    }

    let mut placeholders = Placeholder::default();
    let in_placeholders = (0..query.key_count).map(|_| placeholders.next()).collect::<Vec<_>>().join(", ");
    let mut sql = format!(
        "SELECT {} FROM {} LEFT JOIN {} ON {}.{} = {}.{}",
        relation_join_fields(&query).join(", "),
        query.parent_table,
        query.child_table,
        query.parent_table,
        query.parent_field,
        query.child_table,
        query.child_field,
    );
    let params = append_join_conditions(
        &mut sql,
        query.parent_table,
        query.parent_field,
        query.child_table,
        in_placeholders,
        query.query.conditions,
        &mut placeholders,
    );
    append_order_by(&mut sql, query.query.order_by, Some(query.child_table));

    (sql, params)
}

fn compile_partitioned_relation_batch_query(
    query: RelationBatchQuery,
    select_fields: String,
) -> (String, Vec<DinocoValue>) {
    let table = query.query.from;
    let partition_field = format!("{table}.{}", query.relation_key_field);
    let order_by = relation_partition_order_by(table, query.query.order_by, &partition_field);
    let mut inner_sql = format!(
        "SELECT {select_fields}, ROW_NUMBER() OVER (PARTITION BY {partition_field} ORDER BY {order_by}) AS __dinoco_row_num FROM {table}",
    );
    let mut placeholders = Placeholder::default();
    let mut params = append_conditions(&mut inner_sql, query.query.conditions, Some(table), &mut placeholders);

    append_row_window(&mut params, &mut inner_sql, query.query.skip, query.query.limit, &mut placeholders)
}

fn compile_partitioned_relation_join_query(query: RelationJoinQuery) -> (String, Vec<DinocoValue>) {
    let mut placeholders = Placeholder::default();
    let in_placeholders = (0..query.key_count).map(|_| placeholders.next()).collect::<Vec<_>>().join(", ");
    let partition_field = format!("{}.{}", query.parent_table, query.parent_field);
    let order_by = relation_partition_order_by(
        query.child_table,
        query.query.order_by.clone(),
        &format!("{}.{}", query.child_table, query.child_field),
    );
    let mut fields = relation_join_fields(&query);
    fields.push(format!("ROW_NUMBER() OVER (PARTITION BY {partition_field} ORDER BY {order_by}) AS __dinoco_row_num"));

    let mut inner_sql = format!(
        "SELECT {} FROM {} LEFT JOIN {} ON {}.{} = {}.{}",
        fields.join(", "),
        query.parent_table,
        query.child_table,
        query.parent_table,
        query.parent_field,
        query.child_table,
        query.child_field,
    );
    let mut params = append_join_conditions(
        &mut inner_sql,
        query.parent_table,
        query.parent_field,
        query.child_table,
        in_placeholders,
        query.query.conditions,
        &mut placeholders,
    );

    append_row_window(&mut params, &mut inner_sql, query.query.skip, query.query.limit, &mut placeholders)
}

fn append_row_window(
    params: &mut Vec<DinocoValue>,
    inner_sql: &mut str,
    skip: i32,
    limit: i32,
    placeholders: &mut Placeholder,
) -> (String, Vec<DinocoValue>) {
    let mut row_conditions = Vec::new();
    let skip = skip.max(0);

    if skip > 0 {
        row_conditions.push(format!("__dinoco_row_num > {}", placeholders.next()));
        params.push(DinocoValue::Integer(skip as i64));
    }

    if limit >= 0 {
        row_conditions.push(format!("__dinoco_row_num <= {}", placeholders.next()));
        params.push(DinocoValue::Integer((skip + limit) as i64));
    }

    let mut sql = format!("SELECT * FROM ({inner_sql})");

    if !row_conditions.is_empty() {
        sql.push_str(" WHERE ");
        sql.push_str(&row_conditions.join(" AND "));
    }

    sql.push_str(" ORDER BY __dinoco_relation_key, __dinoco_row_num");

    (sql, std::mem::take(params))
}

fn relation_join_fields(query: &RelationJoinQuery) -> Vec<String> {
    let mut fields =
        query.query.fields.iter().map(|field| format!("{}.{}", query.child_table, field)).collect::<Vec<_>>();

    fields.push(format!("{}.{}", query.child_table, query.child_field));
    fields.push(format!("{}.{} AS __dinoco_relation_key", query.parent_table, query.parent_field));

    fields
}

fn append_find_tail(
    sql: &mut String,
    conditions: Vec<FindWhere>,
    order_by: Option<FindOrderBy>,
    limit: i32,
    skip: i32,
    qualifier: Option<&str>,
    placeholders: &mut Placeholder,
) -> Vec<DinocoValue> {
    let mut params = append_conditions(sql, conditions, qualifier, placeholders);

    append_order_by(sql, order_by, qualifier);

    if limit >= 0 {
        sql.push_str(" LIMIT ");
        sql.push_str(&placeholders.next());
        params.push(DinocoValue::Integer(limit as i64));
    }

    if skip >= 0 {
        sql.push_str(" OFFSET ");
        sql.push_str(&placeholders.next());
        params.push(DinocoValue::Integer(skip as i64));
    }

    params
}

fn append_join_conditions(
    sql: &mut String,
    parent_table: &str,
    parent_field: &str,
    child_table: &str,
    placeholders_sql: String,
    child_conditions: Vec<FindWhere>,
    placeholders: &mut Placeholder,
) -> Vec<DinocoValue> {
    let mut conditions = vec![format!("{parent_table}.{parent_field} IN ({placeholders_sql})")];
    let mut params = Vec::new();

    collect_conditions(&mut conditions, &mut params, child_conditions, Some(child_table), placeholders);

    sql.push_str(" WHERE ");
    sql.push_str(&conditions.join(" AND "));

    params
}

fn append_conditions(
    sql: &mut String,
    conditions: Vec<FindWhere>,
    qualifier: Option<&str>,
    placeholders: &mut Placeholder,
) -> Vec<DinocoValue> {
    let mut params = Vec::new();
    let mut sql_conditions = Vec::new();

    collect_conditions(&mut sql_conditions, &mut params, conditions, qualifier, placeholders);

    if !sql_conditions.is_empty() {
        sql.push_str(" WHERE ");
        sql.push_str(&sql_conditions.join(" AND "));
    }

    params
}

fn append_and_conditions(
    sql: &mut String,
    params: &mut Vec<DinocoValue>,
    conditions: Vec<FindWhere>,
    qualifier: Option<&str>,
    placeholders: &mut Placeholder,
) {
    let mut sql_conditions = Vec::new();

    collect_conditions(&mut sql_conditions, params, conditions, qualifier, placeholders);

    if !sql_conditions.is_empty() {
        sql.push_str(" AND ");
        sql.push_str(&sql_conditions.join(" AND "));
    }
}

fn collect_conditions(
    sql_conditions: &mut Vec<String>,
    params: &mut Vec<DinocoValue>,
    conditions: Vec<FindWhere>,
    qualifier: Option<&str>,
    placeholders: &mut Placeholder,
) {
    for condition in conditions {
        match condition {
            FindWhere::Eq(field, value) => {
                push_binary(sql_conditions, params, field, "=", value, qualifier, placeholders)
            }
            FindWhere::Neq(field, value) => {
                push_binary(sql_conditions, params, field, "!=", value, qualifier, placeholders)
            }
            FindWhere::Gt(field, value) => {
                push_binary(sql_conditions, params, field, ">", value, qualifier, placeholders)
            }
            FindWhere::Gte(field, value) => {
                push_binary(sql_conditions, params, field, ">=", value, qualifier, placeholders)
            }
            FindWhere::Lt(field, value) => {
                push_binary(sql_conditions, params, field, "<", value, qualifier, placeholders)
            }
            FindWhere::Lte(field, value) => {
                push_binary(sql_conditions, params, field, "<=", value, qualifier, placeholders)
            }
            FindWhere::Batch(field, values) => {
                if values.is_empty() {
                    sql_conditions.push("1 = 0".to_string());
                } else {
                    let placeholder_sql = (0..values.len()).map(|_| placeholders.next()).collect::<Vec<_>>().join(", ");
                    sql_conditions.push(format!("{} IN ({placeholder_sql})", qualify_field(field, qualifier)));
                    params.extend(values);
                }
            }
            FindWhere::Null(field) => {
                sql_conditions.push(format!("{} IS NULL", qualify_field(field, qualifier)));
            }
            FindWhere::NotNull(field) => {
                sql_conditions.push(format!("{} IS NOT NULL", qualify_field(field, qualifier)));
            }
        }
    }
}

fn push_binary(
    sql_conditions: &mut Vec<String>,
    params: &mut Vec<DinocoValue>,
    field: &'static str,
    op: &str,
    value: DinocoValue,
    qualifier: Option<&str>,
    placeholders: &mut Placeholder,
) {
    sql_conditions.push(format!("{} {op} {}", qualify_field(field, qualifier), placeholders.next()));
    params.push(value);
}

fn append_order_by(sql: &mut String, order_by: Option<FindOrderBy>, qualifier: Option<&str>) {
    if let Some(order_by) = order_by {
        let (field, direction) = match order_by {
            FindOrderBy::Asc(field) => (field, "ASC"),
            FindOrderBy::Desc(field) => (field, "DESC"),
        };

        sql.push_str(" ORDER BY ");
        sql.push_str(&qualify_field(field, qualifier));
        sql.push(' ');
        sql.push_str(direction);
    }
}

fn relation_partition_order_by(table: &str, order_by: Option<FindOrderBy>, fallback: &str) -> String {
    let Some(order_by) = order_by else {
        return fallback.to_string();
    };

    let (field, direction) = match order_by {
        FindOrderBy::Asc(field) => (field, "ASC"),
        FindOrderBy::Desc(field) => (field, "DESC"),
    };

    format!("{} {direction}", qualify_field(field, Some(table)))
}

fn qualify_field(field: &str, qualifier: Option<&str>) -> String {
    match qualifier {
        Some(qualifier) if !field.contains('.') => format!("{qualifier}.{field}"),
        _ => field.to_string(),
    }
}

#[derive(Default)]
struct Placeholder {
    index: usize,
}

impl Placeholder {
    fn next(&mut self) -> String {
        self.index += 1;
        format!("${}", self.index)
    }
}
