use dinoco_engine::{
    CreateIndexMigration, CreateTableMigration, DinocoAdapter, DinocoSqlCompiler, DropIndexMigration, FindQuery,
    InsertQuery, MigrationColumn, MigrationColumnType, MigrationDefault, MigrationForeignKey, MigrationIndex,
    MigrationIndexKind, ReferentialAction, SqliteAdapter,
};

#[tokio::test]
async fn sqlite_adapter_compiles_migration_sql() -> anyhow::Result<()> {
    let adapter = SqliteAdapter::new(":memory:".to_string()).await.map_err(anyhow::Error::msg)?;
    let sql = adapter.compile_create_table_migration(CreateTableMigration {
        table: "account".to_string(),
        if_not_exists: true,
        columns: vec![
            MigrationColumn {
                name: "id".to_string(),
                ty: MigrationColumnType::String,
                primary_key: true,
                unique: false,
                nullable: false,
                default: None,
            },
            MigrationColumn {
                name: "is_active".to_string(),
                ty: MigrationColumnType::Boolean,
                primary_key: false,
                unique: false,
                nullable: false,
                default: Some(MigrationDefault::Boolean(false)),
            },
        ],
        foreign_keys: Vec::new(),
    });

    assert!(sql.contains("CREATE TABLE IF NOT EXISTS account"));
    assert!(sql.contains("id TEXT PRIMARY KEY NOT NULL"));
    assert!(sql.contains("is_active BOOLEAN NOT NULL DEFAULT 0"));

    Ok(())
}

#[tokio::test]
async fn sqlite_quotes_reserved_identifiers_in_migrations_and_queries() -> anyhow::Result<()> {
    let adapter = SqliteAdapter::new(":memory:".to_string()).await.map_err(anyhow::Error::msg)?;
    let sql = adapter.compile_create_table_migration(CreateTableMigration {
        table: "systems".to_string(),
        if_not_exists: true,
        columns: vec![
            MigrationColumn {
                name: "id".to_string(),
                ty: MigrationColumnType::Integer,
                primary_key: true,
                unique: false,
                nullable: false,
                default: None,
            },
            MigrationColumn {
                name: "group".to_string(),
                ty: MigrationColumnType::String,
                primary_key: false,
                unique: false,
                nullable: false,
                default: None,
            },
        ],
        foreign_keys: Vec::new(),
    });

    assert!(sql.contains("\"group\" TEXT NOT NULL"), "{sql}");
    adapter.execute(&sql, &[]).await?;

    let (insert, params) = adapter.compile_insert_query(InsertQuery {
        table: "systems",
        fields: vec!["id", "group"],
        rows: vec![vec![1_i64.into(), "admin".into()]],
        returning: None,
    });
    assert!(insert.contains("(id, \"group\")"), "{insert}");
    adapter.execute(&insert, &params).await?;

    let (select, params) = adapter.compile_find_query(FindQuery::new(&["id", "group"], "systems", -1, -1));
    assert!(select.contains("SELECT id, \"group\" FROM systems"), "{select}");
    let rows = adapter.query::<dinoco_engine::SingleIdRow>(&select, &params).await?;
    assert_eq!(rows.len(), 1);

    Ok(())
}

#[tokio::test]
async fn sqlite_adapter_compiles_foreign_key_actions() -> anyhow::Result<()> {
    let adapter = SqliteAdapter::new(":memory:".to_string()).await.map_err(anyhow::Error::msg)?;
    let sql = adapter.compile_create_table_migration(CreateTableMigration {
        table: "post".to_string(),
        if_not_exists: true,
        columns: vec![MigrationColumn {
            name: "user_id".to_string(),
            ty: MigrationColumnType::Integer,
            primary_key: false,
            unique: false,
            nullable: true,
            default: None,
        }],
        foreign_keys: vec![MigrationForeignKey {
            name: "fk_post_user_id".to_string(),
            columns: vec!["user_id".to_string()],
            references_table: "user".to_string(),
            references_columns: vec!["id".to_string()],
            on_update: ReferentialAction::Restrict,
            on_delete: ReferentialAction::Cascade,
        }],
    });

    assert!(sql.contains("CONSTRAINT fk_post_user_id FOREIGN KEY (user_id) REFERENCES user (id)"));
    assert!(sql.contains("ON UPDATE RESTRICT ON DELETE CASCADE"));

    Ok(())
}

#[tokio::test]
async fn sqlite_adapter_compiles_and_applies_indexes() -> anyhow::Result<()> {
    let adapter = SqliteAdapter::new(":memory:".to_string()).await.map_err(anyhow::Error::msg)?;
    adapter
        .execute(
            &adapter.compile_create_table_migration(CreateTableMigration {
                table: "account".to_string(),
                if_not_exists: false,
                columns: vec![MigrationColumn {
                    name: "email".to_string(),
                    ty: MigrationColumnType::String,
                    primary_key: false,
                    unique: false,
                    nullable: false,
                    default: None,
                }],
                foreign_keys: Vec::new(),
            }),
            &[],
        )
        .await?;
    let index = MigrationIndex {
        name: "idx_account_email".to_string(),
        columns: vec!["email".to_string()],
        automatic: false,
        kind: MigrationIndexKind::Standard,
    };
    let create = adapter
        .compile_create_index_migration(CreateIndexMigration { table: "account".to_string(), index: index.clone() });
    assert_eq!(create, "CREATE INDEX idx_account_email ON account (email);");
    adapter.execute(&create, &[]).await?;

    let drop = adapter.compile_drop_index_migration(DropIndexMigration { table: "account".to_string(), index });
    assert_eq!(drop, "DROP INDEX idx_account_email;");
    adapter.execute(&drop, &[]).await?;

    Ok(())
}
