use dinoco_engine::{
    DeleteQuery, DinocoAdapter, DinocoSqlCompiler, DinocoValue, FindOrderBy, FindQuery, FindWhere, InsertQuery,
    ManyToManyMatch, MySqlAdapter, PostgresAdapter, RelationJoinQuery, SqliteAdapter, UpdateOperation, UpdateQuery,
    UpdateSet,
};

#[tokio::test]
async fn relation_join_compilers_alias_both_sides_of_self_relations() -> anyhow::Result<()> {
    let sqlite = SqliteAdapter::new(":memory:".to_string()).await.map_err(anyhow::Error::msg)?;
    let postgres = PostgresAdapter::pgbouncer("postgres://postgres:postgres@localhost/postgres").await?;
    let mysql = MySqlAdapter::new("mysql://root:root@localhost/mysql");
    let query = || RelationJoinQuery {
        query: FindQuery {
            fields: &["id", "label", "parent_id"],
            from: "topic_node",
            conditions: vec![FindWhere::Eq("label", DinocoValue::String("Root".to_string()))],
            limit: -1,
            skip: -1,
            order_by: None,
        },
        parent_table: "topic_node",
        child_table: "topic_node",
        parent_field: "parent_id",
        child_field: "id",
        key_count: 1,
    };

    for sql in [
        sqlite.compile_relation_join_query(query()).0,
        mysql.compile_relation_join_query(query()).0,
        postgres.compile_relation_join_query(query()).0,
    ] {
        assert!(sql.contains("topic_node AS __dinoco_parent LEFT JOIN topic_node AS __dinoco_child"), "{sql}");
        assert!(sql.contains("__dinoco_parent.parent_id = __dinoco_child.id"), "{sql}");
        assert!(sql.contains("__dinoco_child.label"), "{sql}");
    }

    Ok(())
}

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
        table: "account",
        sets: vec![
            UpdateSet { field: "balance", value: DinocoValue::Integer(100), operation: UpdateOperation::Decrement },
            UpdateSet { field: "total", value: DinocoValue::Integer(100), operation: UpdateOperation::Increment },
            UpdateSet { field: "multiplier", value: DinocoValue::Float(2.0), operation: UpdateOperation::Multiply },
            UpdateSet { field: "ratio", value: DinocoValue::Float(4.0), operation: UpdateOperation::Divide },
        ],
        conditions: vec![FindWhere::Gte("balance", DinocoValue::Integer(100))],
        returning: None,
    });
    assert_eq!(
        sql,
        "UPDATE account SET balance = balance - ?, total = total + ?, multiplier = multiplier * ?, ratio = ratio / ? WHERE balance >= ?"
    );
    assert_eq!(params.len(), 5);

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
    let postgres = PostgresAdapter::pgbouncer("postgres://postgres:postgres@localhost/postgres").await?;
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

    Ok(())
}

#[tokio::test]
async fn compilers_use_native_fulltext_and_sqlite_like_fallback() -> anyhow::Result<()> {
    let sqlite = SqliteAdapter::new(":memory:".to_string()).await.map_err(anyhow::Error::msg)?;
    let postgres = PostgresAdapter::pgbouncer("postgres://postgres:postgres@localhost/postgres").await?;
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

#[tokio::test]
async fn compilers_translate_many_to_many_virtual_key_filters_into_join_table_subqueries() -> anyhow::Result<()> {
    let sqlite = SqliteAdapter::new(":memory:".to_string()).await.map_err(anyhow::Error::msg)?;
    let postgres = PostgresAdapter::pgbouncer("postgres://postgres:postgres@localhost/postgres").await?;
    let mysql = MySqlAdapter::new("mysql://root:root@localhost/mysql");

    let query = |negated: bool, predicate: FindWhere| FindQuery {
        fields: &["id", "name"],
        from: "relation_system",
        conditions: vec![FindWhere::ManyToMany(ManyToManyMatch {
            local_key: "id",
            join_table: "_relation_business_to_relation_system",
            join_local_field: "system_id",
            join_target_field: "business_id",
            negated,
            predicate: Box::new(predicate),
        })],
        limit: -1,
        skip: -1,
        order_by: None,
    };
    let string = |value: &str| DinocoValue::String(value.to_string());

    // `eq` keeps the predicate inside a membership subquery over the pivot.
    let (sqlite_sql, sqlite_params) =
        sqlite.compile_find_query(query(false, FindWhere::Eq("business_id", string("business-a"))));
    assert_eq!(
        sqlite_sql,
        "SELECT id, name FROM relation_system WHERE id IN (SELECT system_id FROM _relation_business_to_relation_system WHERE business_id = ?)"
    );
    assert_eq!(sqlite_params, [string("business-a")]);

    let (mysql_sql, _) = mysql.compile_find_query(query(false, FindWhere::Eq("business_id", string("business-a"))));
    assert_eq!(mysql_sql, sqlite_sql);

    let (postgres_sql, postgres_params) =
        postgres.compile_find_query(query(false, FindWhere::Eq("business_id", string("business-a"))));
    assert_eq!(
        postgres_sql,
        "SELECT id, name FROM relation_system WHERE id IN (SELECT system_id FROM _relation_business_to_relation_system WHERE business_id = $1)"
    );
    assert_eq!(postgres_params, [string("business-a")]);

    // Any predicate shape works — here a range comparison on the pivot's target column.
    let (sqlite_range_sql, sqlite_range_params) =
        sqlite.compile_find_query(query(false, FindWhere::Gt("business_id", string("business-m"))));
    assert_eq!(
        sqlite_range_sql,
        "SELECT id, name FROM relation_system WHERE id IN (SELECT system_id FROM _relation_business_to_relation_system WHERE business_id > ?)"
    );
    assert_eq!(sqlite_range_params, [string("business-m")]);

    // Negated membership (`neq`/`not_in`) renders `NOT IN` and keeps sequential placeholders.
    let (postgres_negated_sql, postgres_negated_params) = postgres.compile_find_query(query(
        true,
        FindWhere::Batch("business_id", vec![string("business-a"), string("business-b")]),
    ));
    assert_eq!(
        postgres_negated_sql,
        "SELECT id, name FROM relation_system WHERE id NOT IN (SELECT system_id FROM _relation_business_to_relation_system WHERE business_id IN ($1, $2))"
    );
    assert_eq!(postgres_negated_params, [string("business-a"), string("business-b")]);

    // An empty `batch([])` predicate degrades to `1 = 0` inside the subquery.
    let (sqlite_empty_sql, sqlite_empty_params) =
        sqlite.compile_find_query(query(false, FindWhere::Batch("business_id", vec![])));
    assert_eq!(
        sqlite_empty_sql,
        "SELECT id, name FROM relation_system WHERE id IN (SELECT system_id FROM _relation_business_to_relation_system WHERE 1 = 0)"
    );
    assert!(sqlite_empty_params.is_empty());

    // The membership subquery composes with sibling conditions and keeps binding order.
    let (postgres_combined_sql, postgres_combined_params) = postgres.compile_find_query(FindQuery {
        fields: &["id"],
        from: "relation_system",
        conditions: vec![
            FindWhere::Eq("name", string("ERP")),
            FindWhere::ManyToMany(ManyToManyMatch {
                local_key: "id",
                join_table: "_relation_business_to_relation_system",
                join_local_field: "system_id",
                join_target_field: "business_id",
                negated: false,
                predicate: Box::new(FindWhere::Eq("business_id", string("business-a"))),
            }),
        ],
        limit: -1,
        skip: -1,
        order_by: None,
    });
    assert_eq!(
        postgres_combined_sql,
        "SELECT id FROM relation_system WHERE name = $1 AND id IN (SELECT system_id FROM _relation_business_to_relation_system WHERE business_id = $2)"
    );
    assert_eq!(postgres_combined_params, [string("ERP"), string("business-a")]);

    Ok(())
}
