use dinoco_engine::{
    DeleteStatement, DinocoValue, Expression, MySqlDialect, OrderDirection, PostgresDialect, QueryBuilder,
    SelectStatement, SqliteDialect, UpdateBatchItem, UpdateStatement,
};

#[test]
fn sqlite_query_builder_builds_select_with_filters_order_and_pagination() {
    let dialect = SqliteDialect;
    let statement = SelectStatement::new()
        .select(&["id", "email"])
        .from("users")
        .condition(Expression::Column("active".to_string()).eq(true))
        .order_by("id", OrderDirection::Desc)
        .limit(5)
        .skip(10);

    let (sql, params) = dialect.build_select(&statement);

    assert_eq!(
        sql,
        "SELECT \"id\", \"email\" FROM \"users\" WHERE (\"active\" = ?1) ORDER BY \"id\" DESC LIMIT ?2 OFFSET ?3"
    );
    assert_eq!(params, vec![DinocoValue::Boolean(true), DinocoValue::Integer(5), DinocoValue::Integer(10)]);
}

#[test]
fn postgres_query_builder_builds_select_with_filters_order_and_pagination() {
    let dialect = PostgresDialect;
    let statement = SelectStatement::new()
        .select(&["id", "email"])
        .from("users")
        .condition(Expression::Column("active".to_string()).eq(true))
        .order_by("id", OrderDirection::Desc)
        .limit(5)
        .skip(10);

    let (sql, params) = dialect.build_select(&statement);

    assert_eq!(
        sql,
        "SELECT \"id\", \"email\" FROM \"users\" WHERE (\"active\" = $1) ORDER BY \"id\" DESC LIMIT $2 OFFSET $3"
    );
    assert_eq!(params, vec![DinocoValue::Boolean(true), DinocoValue::Integer(5), DinocoValue::Integer(10)]);
}

#[test]
fn postgres_query_builder_casts_native_enum_params() {
    let dialect = PostgresDialect;
    let statement = SelectStatement::new().select(&["id", "role"]).from("users").condition(
        Expression::Column("role".to_string()).eq(DinocoValue::Enum("Role".to_string(), "ADMIN".to_string())),
    );

    let (sql, params) = dialect.build_select(&statement);

    assert_eq!(sql, "SELECT \"id\", \"role\" FROM \"users\" WHERE (\"role\" = $1::\"Role\")");
    assert_eq!(params, vec![DinocoValue::Enum("Role".to_string(), "ADMIN".to_string())]);
}

#[test]
fn mysql_query_builder_builds_select_with_filters_order_and_pagination() {
    let dialect = MySqlDialect;
    let statement = SelectStatement::new()
        .select(&["id", "email"])
        .from("users")
        .condition(Expression::Column("active".to_string()).eq(true))
        .order_by("id", OrderDirection::Desc)
        .limit(5)
        .skip(10);

    let (sql, params) = dialect.build_select(&statement);

    assert_eq!(sql, "SELECT `id`, `email` FROM `users` WHERE (`active` = ?) ORDER BY `id` DESC LIMIT ? OFFSET ?");
    assert_eq!(params, vec![DinocoValue::Boolean(true), DinocoValue::Integer(5), DinocoValue::Integer(10)]);
}

#[test]
fn sqlite_query_builder_builds_count_from_filtered_paginated_select() {
    let dialect = SqliteDialect;
    let statement = SelectStatement::new()
        .select(&["id", "email"])
        .from("users")
        .condition(Expression::Column("active".to_string()).eq(true))
        .order_by("id", OrderDirection::Desc)
        .limit(5)
        .skip(10);

    let (sql, params) = dialect.build_count(&statement);

    assert_eq!(
        sql,
        "SELECT COUNT(*) FROM (SELECT \"id\", \"email\" FROM \"users\" WHERE (\"active\" = ?1) ORDER BY \"id\" DESC LIMIT ?2 OFFSET ?3) AS \"__dinoco_count\""
    );
    assert_eq!(params, vec![DinocoValue::Boolean(true), DinocoValue::Integer(5), DinocoValue::Integer(10)]);
}

#[test]
fn postgres_query_builder_builds_count_from_filtered_paginated_select() {
    let dialect = PostgresDialect;
    let statement = SelectStatement::new()
        .select(&["id", "email"])
        .from("users")
        .condition(Expression::Column("active".to_string()).eq(true))
        .order_by("id", OrderDirection::Desc)
        .limit(5)
        .skip(10);

    let (sql, params) = dialect.build_count(&statement);

    assert_eq!(
        sql,
        "SELECT COUNT(*) FROM (SELECT \"id\", \"email\" FROM \"users\" WHERE (\"active\" = $1) ORDER BY \"id\" DESC LIMIT $2 OFFSET $3) AS \"__dinoco_count\""
    );
    assert_eq!(params, vec![DinocoValue::Boolean(true), DinocoValue::Integer(5), DinocoValue::Integer(10)]);
}

#[test]
fn mysql_query_builder_builds_count_from_filtered_paginated_select() {
    let dialect = MySqlDialect;
    let statement = SelectStatement::new()
        .select(&["id", "email"])
        .from("users")
        .condition(Expression::Column("active".to_string()).eq(true))
        .order_by("id", OrderDirection::Desc)
        .limit(5)
        .skip(10);

    let (sql, params) = dialect.build_count(&statement);

    assert_eq!(
        sql,
        "SELECT COUNT(*) FROM (SELECT `id`, `email` FROM `users` WHERE (`active` = ?) ORDER BY `id` DESC LIMIT ? OFFSET ?) AS `__dinoco_count`"
    );
    assert_eq!(params, vec![DinocoValue::Boolean(true), DinocoValue::Integer(5), DinocoValue::Integer(10)]);
}

#[test]
fn sqlite_query_builder_builds_select_with_in_and_not_in_filters() {
    let dialect = SqliteDialect;
    let statement = SelectStatement::new()
        .select(&["id", "email"])
        .from("users")
        .condition(
            Expression::Column("id".to_string()).in_values(vec![DinocoValue::Integer(1), DinocoValue::Integer(2)]),
        )
        .condition(
            Expression::Column("email".to_string())
                .not_in_values(vec![DinocoValue::from("blocked@dinoco.dev"), DinocoValue::from("hidden@dinoco.dev")]),
        );

    let (sql, params) = dialect.build_select(&statement);

    assert_eq!(
        sql,
        "SELECT \"id\", \"email\" FROM \"users\" WHERE (\"id\" IN (?1, ?2)) AND (\"email\" NOT IN (?3, ?4))"
    );
    assert_eq!(
        params,
        vec![
            DinocoValue::Integer(1),
            DinocoValue::Integer(2),
            DinocoValue::from("blocked@dinoco.dev"),
            DinocoValue::from("hidden@dinoco.dev"),
        ]
    );
}

#[test]
fn sqlite_query_builder_builds_empty_in_and_not_in_filters() {
    let dialect = SqliteDialect;
    let statement = SelectStatement::new()
        .from("users")
        .condition(Expression::Column("id".to_string()).in_values(vec![]))
        .condition(Expression::Column("email".to_string()).not_in_values(vec![]));

    let (sql, params) = dialect.build_select(&statement);

    assert_eq!(sql, "SELECT * FROM \"users\" WHERE (1 = 0) AND (1 = 1)");
    assert!(params.is_empty());
}

#[test]
fn sqlite_query_builder_builds_batched_update() {
    let dialect = SqliteDialect;
    let statement = UpdateStatement::new().table("users").batch(UpdateBatchItem {
        conditions: vec![Expression::Column("id".to_string()).eq(1_i64)],
        values: vec![("email".to_string(), DinocoValue::from("updated@email.com"))],
    });

    let (sql, params) = dialect.build_update(&statement);

    assert_eq!(
        sql,
        "UPDATE \"users\" SET \"email\" = CASE WHEN (\"id\" = ?1) THEN ?2 ELSE \"email\" END WHERE ((\"id\" = ?3))"
    );
    assert_eq!(params, vec![DinocoValue::Integer(1), DinocoValue::from("updated@email.com"), DinocoValue::Integer(1),]);
}

#[test]
fn sqlite_query_builder_builds_atomic_find_and_update_statement() {
    let dialect = SqliteDialect;
    let statement = UpdateStatement::new()
        .table("users")
        .target_first_match(&["id"])
        .condition(Expression::Column("email".to_string()).eq("ana@dinoco.dev"))
        .set("name", "Ana Atomic")
        .increment("score", 1.5_f64);

    let (sql, params) = dialect.build_update(&statement);

    assert_eq!(
        sql,
        "UPDATE \"users\" AS \"__dinoco_update\" SET \"name\" = ?1, \"score\" = (\"score\" + ?2) WHERE EXISTS (SELECT 1 FROM (SELECT \"id\" FROM \"users\" WHERE (\"email\" = ?3) LIMIT 1) AS \"__dinoco_target\" WHERE \"__dinoco_update\".\"id\" = \"__dinoco_target\".\"id\")"
    );
    assert_eq!(
        params,
        vec![DinocoValue::from("Ana Atomic"), DinocoValue::Float(1.5), DinocoValue::from("ana@dinoco.dev"),]
    );
}

#[test]
fn sqlite_query_builder_composes_multiple_operations_for_same_column() {
    let dialect = SqliteDialect;
    let statement = UpdateStatement::new()
        .table("users")
        .target_first_match(&["id"])
        .condition(Expression::Column("id".to_string()).eq(1_i64))
        .increment("age", 1_i64)
        .multiply("age", 2_i64);

    let (sql, params) = dialect.build_update(&statement);

    assert_eq!(
        sql,
        "UPDATE \"users\" AS \"__dinoco_update\" SET \"age\" = ((\"age\" + ?1) * ?2) WHERE EXISTS (SELECT 1 FROM (SELECT \"id\" FROM \"users\" WHERE (\"id\" = ?3) LIMIT 1) AS \"__dinoco_target\" WHERE \"__dinoco_update\".\"id\" = \"__dinoco_target\".\"id\")"
    );
    assert_eq!(params, vec![DinocoValue::Integer(1), DinocoValue::Integer(2), DinocoValue::Integer(1)]);
}

#[test]
fn sqlite_query_builder_casts_division_to_real_expression() {
    let dialect = SqliteDialect;
    let statement = UpdateStatement::new()
        .table("users")
        .target_first_match(&["id"])
        .condition(Expression::Column("id".to_string()).eq(1_i64))
        .increment("age", 1_i64)
        .division("age", 2_i64);

    let (sql, params) = dialect.build_update(&statement);

    assert_eq!(
        sql,
        "UPDATE \"users\" AS \"__dinoco_update\" SET \"age\" = (CAST((\"age\" + ?1) AS REAL) / ?2) WHERE EXISTS (SELECT 1 FROM (SELECT \"id\" FROM \"users\" WHERE (\"id\" = ?3) LIMIT 1) AS \"__dinoco_target\" WHERE \"__dinoco_update\".\"id\" = \"__dinoco_target\".\"id\")"
    );
    assert_eq!(params, vec![DinocoValue::Integer(1), DinocoValue::Integer(2), DinocoValue::Integer(1)]);
}

#[test]
fn postgres_query_builder_builds_partitioned_select_with_window_function() {
    let dialect = PostgresDialect;
    let statement = SelectStatement::new()
        .select(&["id", "group_id", "score"])
        .from("events")
        .order_by("score", OrderDirection::Desc)
        .limit(2)
        .skip(1);

    let (sql, params) = dialect.build_partitioned_select(&statement, "group_id", "row_num");

    assert_eq!(
        sql,
        "SELECT * FROM (SELECT \"id\", \"group_id\", \"score\", ROW_NUMBER() OVER (PARTITION BY \"group_id\" ORDER BY \"score\" DESC) AS \"row_num\" FROM \"events\") AS \"__dinoco_partitioned\" WHERE \"row_num\" > $1 AND \"row_num\" <= $2 ORDER BY \"group_id\", \"row_num\""
    );
    assert_eq!(params, vec![DinocoValue::Integer(1), DinocoValue::Integer(3)]);
}

#[test]
fn mysql_query_builder_builds_partitioned_select_with_window_function() {
    let dialect = MySqlDialect;
    let statement = SelectStatement::new()
        .select(&["id", "group_id", "score"])
        .from("events")
        .order_by("score", OrderDirection::Desc)
        .limit(2)
        .skip(1);

    let (sql, params) = dialect.build_partitioned_select(&statement, "group_id", "row_num");

    assert_eq!(
        sql,
        "SELECT * FROM (SELECT `id`, `group_id`, `score`, ROW_NUMBER() OVER (PARTITION BY `group_id` ORDER BY `score` DESC) AS `row_num` FROM `events`) AS `__dinoco_partitioned` WHERE `row_num` > ? AND `row_num` <= ? ORDER BY `group_id`, `row_num`"
    );
    assert_eq!(params, vec![DinocoValue::Integer(1), DinocoValue::Integer(3)]);
}

#[test]
fn sqlite_query_builder_builds_partitioned_select_with_window_function() {
    let dialect = SqliteDialect;
    let statement = SelectStatement::new()
        .select(&["id", "group_id", "score"])
        .from("events")
        .order_by("score", OrderDirection::Desc)
        .limit(2)
        .skip(1);

    let (sql, params) = dialect.build_partitioned_select(&statement, "group_id", "row_num");

    assert_eq!(
        sql,
        "SELECT * FROM (SELECT \"id\", \"group_id\", \"score\", ROW_NUMBER() OVER (PARTITION BY \"group_id\" ORDER BY \"score\" DESC) AS \"row_num\" FROM \"events\") AS \"__dinoco_partitioned\" WHERE \"row_num\" > ?1 AND \"row_num\" <= ?2 ORDER BY \"group_id\", \"row_num\""
    );
    assert_eq!(params, vec![DinocoValue::Integer(1), DinocoValue::Integer(3)]);
}

#[test]
fn sqlite_query_builder_builds_delete_without_conditions_for_delete_many() {
    let dialect = SqliteDialect;
    let statement = DeleteStatement::new().from("users");

    let (sql, params) = dialect.build_delete(&statement);

    assert_eq!(sql, "DELETE FROM \"users\"");
    assert!(params.is_empty());
}

#[test]
fn mysql_query_builder_builds_delete_without_conditions_for_delete_many() {
    let dialect = MySqlDialect;
    let statement = DeleteStatement::new().from("users");

    let (sql, params) = dialect.build_delete(&statement);

    assert_eq!(sql, "DELETE FROM `users`");
    assert!(params.is_empty());
}

#[test]
fn postgres_query_builder_builds_delete_without_conditions_for_delete_many() {
    let dialect = PostgresDialect;
    let statement = DeleteStatement::new().from("users");

    let (sql, params) = dialect.build_delete(&statement);

    assert_eq!(sql, "DELETE FROM \"users\"");
    assert!(params.is_empty());
}
