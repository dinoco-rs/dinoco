use dinoco_engine::{
    DinocoAdapter, DinocoSqlCompiler, FindQuery, ManyToManyRelationQuery, RelationBatchQuery, RelationJoinQuery,
    SqliteAdapter,
};

#[tokio::test]
async fn relation_limits_use_window_partitions() -> anyhow::Result<()> {
    let adapter = SqliteAdapter::new(":memory:".to_string()).await.map_err(anyhow::Error::msg)?;

    let (batch_sql, batch_params) = adapter.compile_relation_batch_query(RelationBatchQuery {
        query: FindQuery::new(&["id", "user_id"], "user_token", 2, 1),
        relation_key_field: "user_id",
    });

    assert!(batch_sql.contains("ROW_NUMBER() OVER (PARTITION BY user_token.user_id"));
    assert!(batch_sql.contains("__dinoco_row_num > ?"));
    assert!(batch_sql.contains("__dinoco_row_num <= ?"));
    assert_eq!(batch_params.len(), 2);

    let (join_sql, join_params) = adapter.compile_relation_join_query(RelationJoinQuery {
        query: FindQuery::new(&["id", "email"], "user", 1, 0),
        parent_table: "user_token",
        child_table: "user",
        parent_field: "user_id",
        child_field: "id",
        key_count: 2,
    });

    assert!(join_sql.contains("LEFT JOIN user"));
    assert!(join_sql.contains("ROW_NUMBER() OVER (PARTITION BY user_token.user_id"));
    assert!(join_sql.contains("__dinoco_row_num <= ?"));
    assert_eq!(join_params.len(), 1);

    let (many_to_many_sql, many_to_many_params) =
        adapter.compile_many_to_many_relation_query(ManyToManyRelationQuery {
            query: FindQuery::new(&["id", "name"], "system", 3, 1),
            join_table: "_business_to_system",
            parent_field: "id",
            child_field: "id",
            join_parent_field: "business_id",
            join_child_field: "system_id",
            key_count: 2,
        });

    assert!(many_to_many_sql.contains("INNER JOIN system ON _business_to_system.system_id = system.id"));
    assert!(many_to_many_sql.contains("PARTITION BY _business_to_system.business_id"));
    assert!(many_to_many_sql.contains("_business_to_system.business_id IN (?, ?)"));
    assert_eq!(many_to_many_params.len(), 2);

    Ok(())
}
