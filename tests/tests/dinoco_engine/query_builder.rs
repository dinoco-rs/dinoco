use dinoco_engine::{
    DeleteQuery, DinocoAdapter, DinocoSqlCompiler, DinocoValue, FindOrderBy, FindQuery, FindWhere, InsertQuery,
    SqliteAdapter, UpdateOperation, UpdateQuery, UpdateSet,
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
