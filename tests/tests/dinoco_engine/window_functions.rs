use dinoco_engine::{
    DinocoAdapter, DinocoSqlCompiler, FindQuery, ManyToManyRelationQuery, MySqlAdapter, PostgresAdapter,
    RelationBatchQuery, RelationJoinQuery, RelationOccurrenceQuery, SqliteAdapter,
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

    let occurrence_query = || RelationOccurrenceQuery {
        query: FindQuery::new(&["id", "email"], "user", 2, 1),
        child_field: "id",
        key_count: 2,
    };
    let (occurrence_sql, occurrence_params) = adapter.compile_relation_occurrence_query(occurrence_query());
    assert!(
        occurrence_sql.contains(
            "__dinoco_parent_keys(__dinoco_relation_ordinal, __dinoco_relation_key) AS (VALUES (0, ?), (1, ?))"
        )
    );
    assert!(occurrence_sql.contains("PARTITION BY __dinoco_parent_keys.__dinoco_relation_ordinal"));
    assert_eq!(occurrence_params.len(), 2);

    let many_to_many_query = || ManyToManyRelationQuery {
        query: FindQuery::new(&["id", "name"], "system", 3, 1),
        join_table: "_business_to_system",
        parent_field: "id",
        child_field: "id",
        join_parent_field: "business_id",
        join_child_field: "system_id",
        key_count: 2,
    };
    let (many_to_many_sql, many_to_many_params) = adapter.compile_many_to_many_relation_query(many_to_many_query());

    assert!(many_to_many_sql.contains("INNER JOIN system ON _business_to_system.system_id = system.id"));
    assert!(many_to_many_sql.contains("PARTITION BY __dinoco_parent_keys.__dinoco_relation_ordinal"));
    assert!(many_to_many_sql.contains("__dinoco_parent_keys(__dinoco_relation_ordinal) AS (VALUES (0), (1))"));
    assert!(many_to_many_sql.contains("_business_to_system.business_id = ?"));
    assert_eq!(many_to_many_params.len(), 2);

    let mysql = MySqlAdapter::new("mysql://root:root@localhost/mysql");
    let (mysql_occurrence_sql, mysql_occurrence_params) = mysql.compile_relation_occurrence_query(occurrence_query());
    assert!(mysql_occurrence_sql.contains(
        "WITH __dinoco_parent_keys AS (SELECT 0 AS __dinoco_relation_ordinal, ? AS __dinoco_relation_key UNION ALL SELECT 1, ?)"
    ));
    assert_eq!(mysql_occurrence_params.len(), 2);

    let (mysql_sql, mysql_params) = mysql.compile_many_to_many_relation_query(many_to_many_query());
    assert!(
        mysql_sql.contains("WITH __dinoco_parent_keys AS (SELECT 0 AS __dinoco_relation_ordinal UNION ALL SELECT 1)")
    );
    assert_eq!(mysql_sql.matches("_business_to_system.business_id = ?").count(), 2);
    assert_eq!(mysql_params.len(), 2);

    let postgres = PostgresAdapter::pgbouncer("postgres://postgres:postgres@localhost/postgres").await?;
    let (postgres_occurrence_sql, postgres_occurrence_params) =
        postgres.compile_relation_occurrence_query(occurrence_query());
    assert!(postgres_occurrence_sql.contains(
        "__dinoco_parent_keys(__dinoco_relation_ordinal, __dinoco_relation_key) AS (VALUES (0, $1), (1, $2))"
    ));
    assert!(postgres_occurrence_sql.contains("__dinoco_row_num > $3"));
    assert!(postgres_occurrence_sql.contains("__dinoco_row_num <= $4"));
    assert_eq!(postgres_occurrence_params.len(), 2);

    let (postgres_sql, postgres_params) = postgres.compile_many_to_many_relation_query(many_to_many_query());
    assert!(postgres_sql.contains("__dinoco_parent_keys(__dinoco_relation_ordinal) AS (VALUES (0), (1))"));
    assert!(postgres_sql.contains("_business_to_system.business_id = $1"));
    assert!(postgres_sql.contains("_business_to_system.business_id = $2"));
    assert_eq!(postgres_params.len(), 2);

    Ok(())
}
