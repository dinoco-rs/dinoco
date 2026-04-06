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
