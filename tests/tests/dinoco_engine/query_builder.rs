use dinoco_engine::{
    DeleteQuery, DinocoAdapter, DinocoSqlCompiler, DinocoValue, FindOrderBy, FindQuery, FindWhere, InsertQuery,
    ManyToManyWriteQuery, MySqlAdapter, PostgresAdapter, SqliteAdapter, UpdateOperation, UpdateQuery, UpdateSet,
};

#[tokio::test]
async fn sqlite_compiler_builds_crud_queries() -> anyhow::Result<()> {
    let adapter = SqliteAdapter::new(":memory:".to_string()).await.map_err(anyhow::Error::msg)?;

    let (sql, params) = adapter.compile_find_query(FindQuery {
        fields: &["id", "email"],
        from: "user",
        conditions: vec![FindWhere::Eq("email", DinocoValue::String("a@dinoco.rs".to_string()))],
        limit: 1,
        skip: 2,
        order_by: Some(FindOrderBy::Desc("id")),
    });
    assert_eq!(sql, "SELECT id, email FROM user WHERE email = ? ORDER BY id DESC LIMIT ? OFFSET ?");
    assert_eq!(params.len(), 3);

    let (sql, params) = adapter.compile_insert_query(InsertQuery {
        table: "user",
        fields: vec!["id", "email"],
        rows: vec![vec![DinocoValue::String("1".to_string()), DinocoValue::String("a@dinoco.rs".to_string())]],
        returning: Some(&["id"]),
    });
    assert_eq!(sql, "INSERT INTO user (id, email) VALUES (?, ?) RETURNING id");
    assert_eq!(params.len(), 2);

    let (sql, params) = adapter.compile_update_query(UpdateQuery {
        table: "user",
        sets: vec![UpdateSet {
            field: "email",
            value: DinocoValue::String("b@dinoco.rs".to_string()),
            operation: UpdateOperation::Set,
        }],
        conditions: vec![FindWhere::Eq("id", DinocoValue::String("1".to_string()))],
        returning: Some(&["id", "email"]),
    });
    assert_eq!(sql, "UPDATE user SET email = ? WHERE id = ? RETURNING id, email");
    assert_eq!(params.len(), 2);

    let (sql, params) = adapter.compile_delete_query(DeleteQuery {
        table: "user",
        conditions: vec![FindWhere::Eq("id", DinocoValue::String("1".to_string()))],
        returning: None,
    });
    assert_eq!(sql, "DELETE FROM user WHERE id = ?");
    assert_eq!(params.len(), 1);

    Ok(())
}

#[tokio::test]
async fn compilers_preserve_nested_boolean_where_groups_and_parameter_order() -> anyhow::Result<()> {
    let sqlite = SqliteAdapter::new(":memory:".to_string()).await.map_err(anyhow::Error::msg)?;
    let postgres = PostgresAdapter::direct("postgres://postgres:postgres@localhost/postgres").await?;
    let mysql = MySqlAdapter::new("mysql://root:root@localhost/mysql");

    let condition = || {
        FindWhere::Or(vec![
            FindWhere::And(vec![
                FindWhere::Eq("id", DinocoValue::String("id-1".to_string())),
                FindWhere::Eq("name", DinocoValue::String("matheus-1".to_string())),
            ]),
            FindWhere::Or(vec![
                FindWhere::And(vec![
                    FindWhere::Eq("id", DinocoValue::String("id-2".to_string())),
                    FindWhere::Eq("name", DinocoValue::String("matheus-2".to_string())),
                ]),
                FindWhere::And(vec![
                    FindWhere::Eq("id", DinocoValue::String("id-3".to_string())),
                    FindWhere::Not(Box::new(FindWhere::Eq("name", DinocoValue::String("blocked".to_string())))),
                ]),
            ]),
        ])
    };
    let query = |condition| FindQuery {
        fields: &["id"],
        from: "account",
        conditions: vec![condition],
        limit: 1,
        skip: -1,
        order_by: None,
    };

    let (sqlite_sql, sqlite_params) = sqlite.compile_find_query(query(condition()));
    assert_eq!(
        sqlite_sql,
        "SELECT id FROM account WHERE ((id = ? AND name = ?) OR ((id = ? AND name = ?) OR (id = ? AND NOT (name = ?)))) LIMIT ?"
    );
    assert_eq!(sqlite_params.len(), 7);

    let (mysql_sql, mysql_params) = mysql.compile_find_query(query(condition()));
    assert_eq!(mysql_sql, sqlite_sql);
    assert_eq!(mysql_params, sqlite_params);

    let (postgres_sql, postgres_params) = postgres.compile_find_query(query(condition()));
    assert_eq!(
        postgres_sql,
        "SELECT id FROM account WHERE ((id = $1 AND name = $2) OR ((id = $3 AND name = $4) OR (id = $5 AND NOT (name = $6)))) LIMIT $7"
    );
    assert_eq!(postgres_params, sqlite_params);

    let relation_write = || ManyToManyWriteQuery {
        parent_table: "business",
        join_table: "_business_to_system",
        parent_field: "id",
        join_parent_field: "business_id",
        join_child_field: "system_id",
        child_value: DinocoValue::String("system-1".to_string()),
        parent_conditions: vec![FindWhere::Eq("name", DinocoValue::String("Dinoco".to_string()))],
    };
    let expected_params = [DinocoValue::String("system-1".to_string()), DinocoValue::String("Dinoco".to_string())];

    let (sqlite_sql, sqlite_params) = sqlite.compile_connect_many_to_many_query(relation_write());
    assert_eq!(
        sqlite_sql,
        "INSERT INTO _business_to_system (business_id, system_id) SELECT business.id, ? FROM business WHERE business.name = ?"
    );
    assert_eq!(sqlite_params, expected_params);
    let (mysql_sql, mysql_params) = mysql.compile_connect_many_to_many_query(relation_write());
    assert_eq!(mysql_sql, sqlite_sql);
    assert_eq!(mysql_params, expected_params);
    let (postgres_sql, postgres_params) = postgres.compile_connect_many_to_many_query(relation_write());
    assert_eq!(
        postgres_sql,
        "INSERT INTO _business_to_system (business_id, system_id) SELECT business.id, $1 FROM business WHERE business.name = $2"
    );
    assert_eq!(postgres_params, expected_params);

    let (sqlite_sql, sqlite_params) = sqlite.compile_disconnect_many_to_many_query(relation_write());
    assert_eq!(
        sqlite_sql,
        "DELETE FROM _business_to_system WHERE system_id = ? AND business_id IN (SELECT business.id FROM business WHERE business.name = ?)"
    );
    assert_eq!(sqlite_params, expected_params);
    let (mysql_sql, mysql_params) = mysql.compile_disconnect_many_to_many_query(relation_write());
    assert_eq!(mysql_sql, sqlite_sql);
    assert_eq!(mysql_params, expected_params);
    let (postgres_sql, postgres_params) = postgres.compile_disconnect_many_to_many_query(relation_write());
    assert_eq!(
        postgres_sql,
        "DELETE FROM _business_to_system WHERE system_id = $1 AND business_id IN (SELECT business.id FROM business WHERE business.name = $2)"
    );
    assert_eq!(postgres_params, expected_params);

    Ok(())
}

#[tokio::test]
async fn compilers_use_native_fulltext_and_sqlite_like_fallback() -> anyhow::Result<()> {
    let sqlite = SqliteAdapter::new(":memory:".to_string()).await.map_err(anyhow::Error::msg)?;
    let postgres = PostgresAdapter::direct("postgres://postgres:postgres@localhost/postgres").await?;
    let mysql = MySqlAdapter::new("mysql://root:root@localhost/mysql");
    let query = || FindQuery {
        fields: &["id"],
        from: "article",
        conditions: vec![FindWhere::FullText(&["body"], DinocoValue::String("dinoco rust".to_string()))],
        limit: -1,
        skip: -1,
        order_by: None,
    };

    let (sqlite_sql, sqlite_params) = sqlite.compile_find_query(query());
    assert_eq!(sqlite_sql, "SELECT id FROM article WHERE body LIKE ?");
    assert_eq!(sqlite_params, [DinocoValue::String("%dinoco rust%".to_string())]);

    let (postgres_sql, postgres_params) = postgres.compile_find_query(query());
    assert_eq!(
        postgres_sql,
        "SELECT id FROM article WHERE to_tsvector('simple', COALESCE(body, '')) @@ plainto_tsquery('simple', $1)"
    );
    assert_eq!(postgres_params, [DinocoValue::String("dinoco rust".to_string())]);

    let (mysql_sql, mysql_params) = mysql.compile_find_query(query());
    assert_eq!(mysql_sql, "SELECT id FROM article WHERE MATCH (body) AGAINST (? IN NATURAL LANGUAGE MODE)");
    assert_eq!(mysql_params, [DinocoValue::String("dinoco rust".to_string())]);

    let composite_query = || FindQuery {
        fields: &["id"],
        from: "article",
        conditions: vec![FindWhere::FullText(&["title", "body"], DinocoValue::String("dinoco rust".to_string()))],
        limit: -1,
        skip: -1,
        order_by: None,
    };
    let (sqlite_sql, sqlite_params) = sqlite.compile_find_query(composite_query());
    assert_eq!(sqlite_sql, "SELECT id FROM article WHERE (title LIKE ? OR body LIKE ?)");
    assert_eq!(
        sqlite_params,
        [DinocoValue::String("%dinoco rust%".to_string()), DinocoValue::String("%dinoco rust%".to_string())]
    );

    let (postgres_sql, _) = postgres.compile_find_query(composite_query());
    assert_eq!(
        postgres_sql,
        "SELECT id FROM article WHERE to_tsvector('simple', COALESCE(title, '') || ' ' || COALESCE(body, '')) @@ plainto_tsquery('simple', $1)"
    );

    let (mysql_sql, _) = mysql.compile_find_query(composite_query());
    assert_eq!(mysql_sql, "SELECT id FROM article WHERE MATCH (title, body) AGAINST (? IN NATURAL LANGUAGE MODE)");

    Ok(())
}
