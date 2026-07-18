use crate::{
    DinocoSqlCompiler, DinocoValue, FindOrderBy, FindQuery, FindWhere, RelationBatchQuery, RelationJoinQuery,
    SqliteAdapter,
};

impl DinocoSqlCompiler for SqliteAdapter {
    fn compile_find_query(&self, query: FindQuery) -> (String, Vec<DinocoValue>) {
        let mut sql = format!("SELECT {} FROM {}", query.fields.join(", "), query.from);
        let params = append_find_tail(&mut sql, query.conditions, query.order_by, query.limit, query.skip);

        (sql, params)
    }

    fn compile_relation_batch_query(&self, query: RelationBatchQuery) -> (String, Vec<DinocoValue>) {
        let fields =
            query.query.fields.iter().map(|field| format!("{}.{}", query.query.from, field)).collect::<Vec<_>>();
        let mut select_fields = fields.join(", ");
        select_fields.push_str(", ");
        select_fields.push_str(query.query.from);
        select_fields.push('.');
        select_fields.push_str(query.relation_key_field);
        select_fields.push_str(" AS __dinoco_relation_key");

        if query.query.limit >= 0 || query.query.skip >= 0 {
            return compile_partitioned_relation_batch_query(query, select_fields);
        }

        let mut sql = format!("SELECT {select_fields} FROM {}", query.query.from);
        let params = append_find_tail(
            &mut sql,
            query.query.conditions,
            query.query.order_by,
            query.query.limit,
            query.query.skip,
        );

        (sql, params)
    }

    fn compile_relation_join_query(&self, query: RelationJoinQuery) -> String {
        let placeholders = vec!["?"; query.key_count].join(", ");
        let mut fields =
            query.fields.iter().map(|field| format!("{}.{}", query.child_table, field)).collect::<Vec<_>>();
        fields.push(format!("{}.{}", query.child_table, query.child_field));
        fields.push(format!("{}.{}", query.parent_table, query.parent_field));

        format!(
            "SELECT {} FROM {} LEFT JOIN {} ON {}.{} = {}.{} WHERE {}.{} IN ({placeholders})",
            fields.join(", "),
            query.parent_table,
            query.child_table,
            query.parent_table,
            query.parent_field,
            query.child_table,
            query.child_field,
            query.parent_table,
            query.parent_field,
        )
    }
}

fn compile_partitioned_relation_batch_query(
    query: RelationBatchQuery,
    select_fields: String,
) -> (String, Vec<DinocoValue>) {
    let partition_field = format!("{}.{}", query.query.from, query.relation_key_field);
    let order_by = relation_partition_order_by(&query.query.from, query.query.order_by, &partition_field);
    let mut inner_sql = format!(
        "SELECT {select_fields}, ROW_NUMBER() OVER (PARTITION BY {partition_field} ORDER BY {order_by}) AS __dinoco_row_num FROM {}",
        query.query.from
    );
    let mut params = append_conditions(&mut inner_sql, query.query.conditions);
    let mut row_conditions = Vec::new();
    let skip = query.query.skip.max(0);

    if skip > 0 {
        row_conditions.push("__dinoco_row_num > ?".to_string());
        params.push(DinocoValue::Integer(skip as i64));
    }

    if query.query.limit >= 0 {
        row_conditions.push("__dinoco_row_num <= ?".to_string());
        params.push(DinocoValue::Integer((skip + query.query.limit) as i64));
    }

    let mut sql = format!("SELECT * FROM ({inner_sql})");

    if !row_conditions.is_empty() {
        sql.push_str(" WHERE ");
        sql.push_str(&row_conditions.join(" AND "));
    }

    sql.push_str(" ORDER BY ");
    sql.push_str("__dinoco_relation_key");
    sql.push_str(", __dinoco_row_num");

    (sql, params)
}

fn append_find_tail(
    sql: &mut String,
    conditions: Vec<FindWhere>,
    order_by: Option<FindOrderBy>,
    limit: i32,
    skip: i32,
) -> Vec<DinocoValue> {
    let mut params = append_conditions(sql, conditions);

    append_order_by(sql, order_by);

    if limit >= 0 {
        sql.push_str(" LIMIT ?");
        params.push(DinocoValue::Integer(limit as i64));
    }

    if skip >= 0 {
        sql.push_str(" OFFSET ?");
        params.push(DinocoValue::Integer(skip as i64));
    }

    params
}

fn append_conditions(sql: &mut String, conditions: Vec<FindWhere>) -> Vec<DinocoValue> {
    let mut params = Vec::new();

    if !conditions.is_empty() {
        let mut sql_conditions = Vec::new();

        for condition in conditions {
            match condition {
                FindWhere::Eq(field, value) => {
                    sql_conditions.push(format!("{field} = ?"));
                    params.push(value);
                }
                FindWhere::Neq(field, value) => {
                    sql_conditions.push(format!("{field} != ?"));
                    params.push(value);
                }
                FindWhere::Gt(field, value) => {
                    sql_conditions.push(format!("{field} > ?"));
                    params.push(value);
                }
                FindWhere::Gte(field, value) => {
                    sql_conditions.push(format!("{field} >= ?"));
                    params.push(value);
                }
                FindWhere::Lt(field, value) => {
                    sql_conditions.push(format!("{field} < ?"));
                    params.push(value);
                }
                FindWhere::Lte(field, value) => {
                    sql_conditions.push(format!("{field} <= ?"));
                    params.push(value);
                }
                FindWhere::Batch(field, values) => {
                    if values.is_empty() {
                        sql_conditions.push("1 = 0".to_string());
                    } else {
                        let placeholders = vec!["?"; values.len()].join(", ");
                        sql_conditions.push(format!("{field} IN ({placeholders})"));
                        params.extend(values);
                    }
                }
                FindWhere::Null(field) => {
                    sql_conditions.push(format!("{field} IS NULL"));
                }
                FindWhere::NotNull(field) => {
                    sql_conditions.push(format!("{field} IS NOT NULL"));
                }
            }
        }

        sql.push_str(" WHERE ");
        sql.push_str(&sql_conditions.join(" AND "));
    }

    params
}

fn append_order_by(sql: &mut String, order_by: Option<FindOrderBy>) {
    if let Some(order_by) = order_by {
        let (field, direction) = match order_by {
            FindOrderBy::Asc(field) => (field, "ASC"),
            FindOrderBy::Desc(field) => (field, "DESC"),
        };

        sql.push_str(" ORDER BY ");
        sql.push_str(field);
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

    if field.contains('.') { format!("{field} {direction}") } else { format!("{table}.{field} {direction}") }
}
