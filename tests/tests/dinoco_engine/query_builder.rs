use dinoco_engine::{
    DeleteQuery, DinocoAdapter, DinocoSqlCompiler, DinocoValue, FindOrderBy, FindQuery, FindWhere, InsertQuery,
    ManyToManyMatch, MySqlAdapter, PostgresAdapter, RelationJoinQuery, RelationJoinTable, RelationMatch,
    RelationOccurrenceQuery, RelationQuantifier, SqliteAdapter, UpdateOperation, UpdateQuery, UpdateSet,
    UpdatedAtField,
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

#[tokio::test]
async fn compilers_assign_updated_at_from_the_database_clock_without_binding_it() -> anyhow::Result<()> {
    let sqlite = SqliteAdapter::new(":memory:".to_string()).await.map_err(anyhow::Error::msg)?;
    let postgres = PostgresAdapter::pgbouncer("postgres://postgres:postgres@localhost/postgres").await?;
    let mysql = MySqlAdapter::new("mysql://root:root@localhost/mysql");
    let query = || UpdateQuery {
        table: "article",
        sets: vec![
            UpdateSet { field: "views", value: DinocoValue::Integer(1), operation: UpdateOperation::Increment },
            UpdatedAtField::timestamp("updated_at").update_set(),
            UpdatedAtField::date("touched_on").update_set(),
            UpdateSet {
                field: "title",
                value: DinocoValue::String("new".to_string()),
                operation: UpdateOperation::Set,
            },
        ],
        conditions: vec![FindWhere::Eq("id", DinocoValue::String("a".to_string()))],
        returning: None,
    };

    let (sql, params) = sqlite.compile_update_query(query());
    assert_eq!(
        sql,
        "UPDATE article SET views = views + ?, updated_at = strftime('%Y-%m-%dT%H:%M:%f+00:00', 'now'), touched_on = date('now'), title = ? WHERE id = ?"
    );
    assert_eq!(
        params,
        vec![DinocoValue::Integer(1), DinocoValue::String("new".to_string()), DinocoValue::String("a".to_string())]
    );

    let (sql, params) = mysql.compile_update_query(query());
    assert_eq!(
        sql,
        "UPDATE article SET views = views + ?, updated_at = UTC_TIMESTAMP(), touched_on = UTC_DATE(), title = ? WHERE id = ?"
    );
    assert_eq!(params.len(), 3);

    let (sql, params) = postgres.compile_update_query(query());
    assert_eq!(
        sql,
        "UPDATE article SET views = views + $1, updated_at = (CURRENT_TIMESTAMP AT TIME ZONE 'UTC'), touched_on = (CURRENT_TIMESTAMP AT TIME ZONE 'UTC')::date, title = $2 WHERE id = $3"
    );
    assert_eq!(params.len(), 3);

    Ok(())
}

#[test]
fn updated_at_operations_are_scalar_but_bind_nothing() {
    for operation in [UpdateOperation::CurrentTimestamp, UpdateOperation::CurrentDate] {
        assert!(operation.is_scalar());
        assert!(!operation.binds_value());
    }
    for operation in [UpdateOperation::Set, UpdateOperation::Increment, UpdateOperation::Divide] {
        assert!(operation.binds_value());
    }

    // A condition on `updated_at` no longer identifies the row after the
    // update, so the MySQL reload drops it instead of reusing a stale value.
    let query = UpdateQuery {
        table: "article",
        sets: vec![
            UpdateSet { field: "title", value: DinocoValue::String("x".to_string()), operation: UpdateOperation::Set },
            UpdatedAtField::timestamp("updated_at").update_set(),
        ],
        conditions: vec![
            FindWhere::Eq("slug", DinocoValue::String("a".to_string())),
            FindWhere::Lt("updated_at", DinocoValue::String("2001-01-01".to_string())),
        ],
        returning: Some(&["slug"]),
    };
    let reload = format!("{:?}", query.post_update_reload_conditions());
    assert!(reload.contains("\"slug\""), "{reload}");
    assert!(!reload.contains("updated_at"), "{reload}");
}

fn relation(
    parent_table: &'static str,
    parent_field: &'static str,
    child_table: &'static str,
    child_field: &'static str,
    quantifier: RelationQuantifier,
    conditions: Vec<FindWhere>,
) -> FindWhere {
    FindWhere::Relation(RelationMatch {
        parent_table,
        parent_field,
        child_table,
        child_field,
        join: None,
        quantifier,
        conditions,
    })
}

fn find(from: &'static str, conditions: Vec<FindWhere>) -> FindQuery {
    FindQuery { fields: &["id"], from, conditions, limit: -1, skip: -1, order_by: None }
}

fn string(value: &str) -> DinocoValue {
    DinocoValue::String(value.to_string())
}

#[tokio::test]
async fn compilers_translate_relation_filters_into_correlated_exists_subqueries() -> anyhow::Result<()> {
    let sqlite = SqliteAdapter::new(":memory:".to_string()).await.map_err(anyhow::Error::msg)?;
    let postgres = PostgresAdapter::pgbouncer("postgres://postgres:postgres@localhost/postgres").await?;
    let mysql = MySqlAdapter::new("mysql://root:root@localhost/mysql");
    let pix = || {
        relation(
            "rf_transaction",
            "payout_id",
            "rf_payout",
            "id",
            RelationQuantifier::Some,
            vec![FindWhere::Eq("method", DinocoValue::Enum("PayoutMethod".to_string(), "pix".to_string()))],
        )
    };

    // The related table is aliased; the outer row is the unaliased queried table.
    let (sql, params) = sqlite.compile_find_query(find("rf_transaction", vec![pix()]));
    assert_eq!(
        sql,
        "SELECT id FROM rf_transaction WHERE EXISTS (SELECT 1 FROM rf_payout AS __dinoco_relation_1 WHERE __dinoco_relation_1.id = rf_transaction.payout_id AND __dinoco_relation_1.method = ?)"
    );
    assert_eq!(params, [DinocoValue::Enum("PayoutMethod".to_string(), "pix".to_string())]);
    assert_eq!(mysql.compile_find_query(find("rf_transaction", vec![pix()])).0, sql);
    assert_eq!(postgres.compile_find_query(find("rf_transaction", vec![pix()])).0, sql.replace('?', "$1"));

    // Nested filters number their aliases by depth and correlate with the
    // enclosing alias; Postgres placeholders follow the textual order.
    let nested = || {
        find(
            "rf_transaction",
            vec![
                FindWhere::Gte("amount", DinocoValue::Integer(10)),
                relation(
                    "rf_transaction",
                    "payout_id",
                    "rf_payout",
                    "id",
                    RelationQuantifier::Some,
                    vec![
                        FindWhere::Eq("method", string("pix")),
                        relation(
                            "rf_payout",
                            "bank_id",
                            "rf_bank",
                            "id",
                            RelationQuantifier::Some,
                            vec![FindWhere::Eq("name", string("Nubank"))],
                        ),
                    ],
                ),
                FindWhere::Lt("amount", DinocoValue::Integer(500)),
            ],
        )
    };
    let (sql, params) = postgres.compile_find_query(nested());
    assert_eq!(
        sql,
        "SELECT id FROM rf_transaction WHERE amount >= $1 AND EXISTS (SELECT 1 FROM rf_payout AS __dinoco_relation_1 WHERE __dinoco_relation_1.id = rf_transaction.payout_id AND __dinoco_relation_1.method = $2 AND EXISTS (SELECT 1 FROM rf_bank AS __dinoco_relation_2 WHERE __dinoco_relation_2.id = __dinoco_relation_1.bank_id AND __dinoco_relation_2.name = $3)) AND amount < $4"
    );
    assert_eq!(params, [DinocoValue::Integer(10), string("pix"), string("Nubank"), DinocoValue::Integer(500)]);
    let (sqlite_sql, sqlite_params) = sqlite.compile_find_query(nested());
    assert_eq!(sqlite_sql, sql.replace("$1", "?").replace("$2", "?").replace("$3", "?").replace("$4", "?"));
    assert_eq!(sqlite_params, params);

    // Quantifiers: `none` negates, `every` negates the rows whose conditions
    // aren't TRUE (so NULL doesn't match), an empty list tests for existence.
    let items = |quantifier, conditions| {
        find(
            "rf_transaction",
            vec![relation("rf_transaction", "id", "rf_item", "transaction_id", quantifier, conditions)],
        )
    };
    let refunded = || vec![FindWhere::Eq("refunded", DinocoValue::Boolean(true))];
    assert_eq!(
        sqlite.compile_find_query(items(RelationQuantifier::None, refunded())).0,
        "SELECT id FROM rf_transaction WHERE NOT EXISTS (SELECT 1 FROM rf_item AS __dinoco_relation_1 WHERE __dinoco_relation_1.transaction_id = rf_transaction.id AND __dinoco_relation_1.refunded = ?)"
    );
    assert_eq!(
        postgres.compile_find_query(items(RelationQuantifier::Every, refunded())).0,
        "SELECT id FROM rf_transaction WHERE NOT EXISTS (SELECT 1 FROM rf_item AS __dinoco_relation_1 WHERE __dinoco_relation_1.transaction_id = rf_transaction.id AND (__dinoco_relation_1.refunded = $1) IS NOT TRUE)"
    );
    assert_eq!(
        mysql.compile_find_query(items(RelationQuantifier::Some, vec![])).0,
        "SELECT id FROM rf_transaction WHERE EXISTS (SELECT 1 FROM rf_item AS __dinoco_relation_1 WHERE __dinoco_relation_1.transaction_id = rf_transaction.id)"
    );
    assert_eq!(
        sqlite.compile_find_query(items(RelationQuantifier::None, vec![])).0,
        "SELECT id FROM rf_transaction WHERE NOT EXISTS (SELECT 1 FROM rf_item AS __dinoco_relation_1 WHERE __dinoco_relation_1.transaction_id = rf_transaction.id)"
    );
    assert_eq!(
        sqlite.compile_find_query(items(RelationQuantifier::Every, vec![])).0,
        "SELECT id FROM rf_transaction WHERE 1 = 1"
    );

    // Many-to-many relations join the related table through the join table.
    let tags = FindWhere::Relation(RelationMatch {
        parent_table: "rf_transaction",
        parent_field: "id",
        child_table: "rf_tag",
        child_field: "id",
        join: Some(RelationJoinTable {
            table: "_rf_tag_to_rf_transaction",
            parent_field: "transaction_id",
            child_field: "tag_id",
        }),
        quantifier: RelationQuantifier::None,
        conditions: vec![FindWhere::Eq("name", string("vip"))],
    });
    assert_eq!(
        mysql.compile_find_query(find("rf_transaction", vec![tags])).0,
        "SELECT id FROM rf_transaction WHERE NOT EXISTS (SELECT 1 FROM _rf_tag_to_rf_transaction AS __dinoco_relation_join_1 INNER JOIN rf_tag AS __dinoco_relation_1 ON __dinoco_relation_1.id = __dinoco_relation_join_1.tag_id WHERE __dinoco_relation_join_1.transaction_id = rf_transaction.id AND __dinoco_relation_1.name = ?)"
    );

    // Relation filters compose with boolean groups like any other condition.
    let (sql, params) = postgres.compile_find_query(find(
        "rf_transaction",
        vec![FindWhere::Or(vec![FindWhere::Not(Box::new(pix())), FindWhere::Eq("amount", DinocoValue::Integer(1))])],
    ));
    assert_eq!(
        sql,
        "SELECT id FROM rf_transaction WHERE (NOT (EXISTS (SELECT 1 FROM rf_payout AS __dinoco_relation_1 WHERE __dinoco_relation_1.id = rf_transaction.payout_id AND __dinoco_relation_1.method = $1)) OR amount = $2)"
    );
    assert_eq!(params.len(), 2);

    Ok(())
}

#[tokio::test]
async fn relation_filters_keep_self_relations_and_qualified_queries_apart() -> anyhow::Result<()> {
    let sqlite = SqliteAdapter::new(":memory:".to_string()).await.map_err(anyhow::Error::msg)?;
    let postgres = PostgresAdapter::pgbouncer("postgres://postgres:postgres@localhost/postgres").await?;
    let mysql = MySqlAdapter::new("mysql://root:root@localhost/mysql");
    let manager_of_manager = || {
        relation(
            "employee",
            "manager_id",
            "employee",
            "id",
            RelationQuantifier::Some,
            vec![relation(
                "employee",
                "manager_id",
                "employee",
                "id",
                RelationQuantifier::Some,
                vec![FindWhere::Eq("name", string("Ana"))],
            )],
        )
    };

    // Each depth has its own alias, so the outer `employee` is never shadowed.
    assert_eq!(
        sqlite.compile_find_query(find("employee", vec![manager_of_manager()])).0,
        "SELECT id FROM employee WHERE EXISTS (SELECT 1 FROM employee AS __dinoco_relation_1 WHERE __dinoco_relation_1.id = employee.manager_id AND EXISTS (SELECT 1 FROM employee AS __dinoco_relation_2 WHERE __dinoco_relation_2.id = __dinoco_relation_1.manager_id AND __dinoco_relation_2.name = ?))"
    );

    // Inside a qualified query the outer row is the query's own qualifier.
    let join_query = RelationJoinQuery {
        query: find("employee", vec![manager_of_manager()]),
        parent_table: "employee",
        child_table: "employee",
        parent_field: "manager_id",
        child_field: "id",
        key_count: 1,
    };
    let (sql, _) = postgres.compile_relation_join_query(join_query);
    assert!(sql.contains("__dinoco_relation_1.id = __dinoco_child.manager_id"), "{sql}");
    assert!(sql.contains("__dinoco_relation_2.id = __dinoco_relation_1.manager_id"), "{sql}");

    let (sql, params) = mysql.compile_relation_occurrence_query(RelationOccurrenceQuery {
        query: find("employee", vec![manager_of_manager()]),
        child_field: "manager_id",
        key_count: 2,
    });
    assert!(sql.contains("WHERE __dinoco_relation_1.id = employee.manager_id"), "{sql}");
    assert_eq!(params, [string("Ana")]);

    // Identifiers are quoted per dialect, aliases included where needed.
    let order =
        || find("Order", vec![relation("Order", "customerId", "Customer", "id", RelationQuantifier::Some, vec![])]);
    assert_eq!(
        sqlite.compile_find_query(order()).0,
        r#"SELECT id FROM "Order" WHERE EXISTS (SELECT 1 FROM "Customer" AS __dinoco_relation_1 WHERE __dinoco_relation_1.id = "Order"."customerId")"#
    );
    assert_eq!(
        mysql.compile_find_query(order()).0,
        "SELECT id FROM `Order` WHERE EXISTS (SELECT 1 FROM `Customer` AS __dinoco_relation_1 WHERE __dinoco_relation_1.id = `Order`.`customerId`)"
    );

    Ok(())
}

#[tokio::test]
async fn mysql_writes_read_their_own_table_through_a_materialized_derived_table() -> anyhow::Result<()> {
    let sqlite = SqliteAdapter::new(":memory:".to_string()).await.map_err(anyhow::Error::msg)?;
    let postgres = PostgresAdapter::pgbouncer("postgres://postgres:postgres@localhost/postgres").await?;
    let mysql = MySqlAdapter::new("mysql://root:root@localhost/mysql");
    let managed_by_ana = || {
        relation(
            "employee",
            "manager_id",
            "employee",
            "id",
            RelationQuantifier::Some,
            vec![FindWhere::Eq("name", string("Ana"))],
        )
    };
    let has_badge = || relation("employee", "id", "badge", "employee_id", RelationQuantifier::None, vec![]);
    let update = |conditions| UpdateQuery {
        table: "employee",
        sets: vec![UpdateSet { field: "name", value: string("Lead"), operation: UpdateOperation::Set }],
        conditions,
        returning: None,
    };

    // MySQL error 1093: the write can't read `employee` straight from a subquery.
    let (sql, params) = mysql.compile_update_query(update(vec![managed_by_ana(), has_badge()]));
    assert_eq!(
        sql,
        "UPDATE employee SET name = ? WHERE EXISTS (SELECT 1 FROM (SELECT DISTINCT __dinoco_relation_1.id AS __dinoco_key FROM employee AS __dinoco_relation_1 WHERE __dinoco_relation_1.name = ?) AS __dinoco_relation_keys_1 WHERE __dinoco_relation_keys_1.__dinoco_key = employee.manager_id) AND NOT EXISTS (SELECT 1 FROM badge AS __dinoco_relation_1 WHERE __dinoco_relation_1.employee_id = employee.id)"
    );
    assert_eq!(params, [string("Lead"), string("Ana")]);

    // Nested in boolean groups, and reached only through a nested relation.
    let through_badge = relation(
        "employee",
        "id",
        "badge",
        "employee_id",
        RelationQuantifier::Every,
        vec![relation("badge", "issuer_id", "employee", "id", RelationQuantifier::Some, vec![])],
    );
    let (sql, _) = mysql.compile_delete_query(DeleteQuery {
        table: "employee",
        conditions: vec![FindWhere::Or(vec![FindWhere::Not(Box::new(managed_by_ana())), through_badge])],
        returning: None,
    });
    assert_eq!(
        sql,
        "DELETE FROM employee WHERE (NOT (EXISTS (SELECT 1 FROM (SELECT DISTINCT __dinoco_relation_1.id AS __dinoco_key FROM employee AS __dinoco_relation_1 WHERE __dinoco_relation_1.name = ?) AS __dinoco_relation_keys_1 WHERE __dinoco_relation_keys_1.__dinoco_key = employee.manager_id)) OR NOT EXISTS (SELECT 1 FROM (SELECT DISTINCT __dinoco_relation_1.employee_id AS __dinoco_key FROM badge AS __dinoco_relation_1 WHERE (EXISTS (SELECT 1 FROM employee AS __dinoco_relation_2 WHERE __dinoco_relation_2.id = __dinoco_relation_1.issuer_id)) IS NOT TRUE) AS __dinoco_relation_keys_1 WHERE __dinoco_relation_keys_1.__dinoco_key = employee.id))"
    );

    // Reads, and the other dialects, keep the plain correlated subquery.
    assert!(!mysql.compile_find_query(find("employee", vec![managed_by_ana()])).0.contains("DISTINCT"));
    assert!(!sqlite.compile_update_query(update(vec![managed_by_ana()])).0.contains("DISTINCT"));
    assert_eq!(
        postgres.compile_update_query(update(vec![managed_by_ana()])).0,
        "UPDATE employee SET name = $1 WHERE EXISTS (SELECT 1 FROM employee AS __dinoco_relation_1 WHERE __dinoco_relation_1.id = employee.manager_id AND __dinoco_relation_1.name = $2)"
    );

    Ok(())
}

#[test]
fn mysql_update_reload_keeps_relation_filters_unless_their_key_changes() {
    let query = |field: &'static str| UpdateQuery {
        table: "rf_transaction",
        sets: vec![UpdateSet { field, value: string("p-ted"), operation: UpdateOperation::Set }],
        conditions: vec![relation("rf_transaction", "payout_id", "rf_payout", "id", RelationQuantifier::Some, vec![])],
        returning: Some(&["id"]),
    };

    let reload = format!("{:?}", query("note").post_update_reload_conditions());
    assert!(reload.contains("Relation"), "{reload}");

    let reload = format!("{:?}", query("payout_id").post_update_reload_conditions());
    assert!(!reload.contains("Relation"), "{reload}");
    assert!(reload.contains("Eq(\"payout_id\""), "{reload}");
}

// What a MySQL `find_and_update` runs inside a transaction: the conditional
// UPDATE, then, since MySQL has no RETURNING, a SELECT of the row by
// `post_update_reload_conditions`.
fn mysql_atomic_update(update: UpdateQuery) -> [(String, Vec<DinocoValue>); 2] {
    let mysql = MySqlAdapter::new("mysql://root:root@localhost/mysql");
    let reload = update.post_update_reload_conditions();
    let (table, returning) = (update.table, update.returning.expect("returning projection"));

    [
        mysql.compile_update_query(UpdateQuery { returning: None, ..update }),
        mysql.compile_find_query(FindQuery {
            fields: returning,
            from: table,
            conditions: reload,
            limit: 1,
            skip: -1,
            order_by: None,
        }),
    ]
}

#[tokio::test]
async fn mysql_atomic_update_compiles_the_update_before_its_compatibility_read() {
    let [(update_sql, _), (reload_sql, reload_params)] = mysql_atomic_update(UpdateQuery {
        table: "business",
        sets: vec![UpdateSet {
            field: "balance",
            value: DinocoValue::Integer(80),
            operation: UpdateOperation::Decrement,
        }],
        conditions: vec![
            FindWhere::Eq("id", DinocoValue::String("business-1".to_string())),
            FindWhere::Gte("balance", DinocoValue::Integer(80)),
        ],
        returning: Some(&["id"]),
    });

    assert_eq!(update_sql, "UPDATE business SET balance = balance - ? WHERE id = ? AND balance >= ?");
    assert_eq!(reload_sql, "SELECT id FROM business WHERE id = ? LIMIT ?");
    assert_eq!(reload_params[0], DinocoValue::String("business-1".to_string()));
}

#[tokio::test]
async fn mysql_atomic_update_reloads_a_changed_filter_by_its_new_set_value() {
    let [(update_sql, _), (reload_sql, reload_params)] = mysql_atomic_update(UpdateQuery {
        table: "document",
        sets: vec![UpdateSet {
            field: "body",
            value: DinocoValue::String("updated body".to_string()),
            operation: UpdateOperation::Set,
        }],
        conditions: vec![FindWhere::FullText(&["body"], DinocoValue::String("original".to_string()))],
        returning: Some(&["id"]),
    });

    assert_eq!(update_sql, "UPDATE document SET body = ? WHERE MATCH (body) AGAINST (? IN NATURAL LANGUAGE MODE)");
    assert_eq!(reload_sql, "SELECT id FROM document WHERE body = ? LIMIT ?");
    assert_eq!(reload_params[0], DinocoValue::String("updated body".to_string()));
}
