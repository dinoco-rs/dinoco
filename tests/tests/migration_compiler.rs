use dinoco_engine::{
    CreateEnumMigration, CreateIndexMigration, CreateTableMigration, DinocoAdapter, DinocoSqlCompiler,
    DropIndexMigration, FindQuery, InsertQuery, MigrationColumn, MigrationColumnType, MigrationDefault,
    MigrationForeignKey, MigrationIndex, MigrationIndexKind, MySqlAdapter, PostgresAdapter, ReferentialAction,
    RenameTableMigration, SqliteAdapter,
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
async fn sqlite_adapter_renames_case_only_legacy_tables_without_losing_rows() -> anyhow::Result<()> {
    let adapter = SqliteAdapter::new(":memory:".to_string()).await.map_err(anyhow::Error::msg)?;
    adapter.execute("CREATE TABLE \"Account\" (id INTEGER PRIMARY KEY, name TEXT NOT NULL)", &[]).await?;
    adapter.execute("INSERT INTO \"Account\" (name) VALUES ('preserved')", &[]).await?;

    let statements = adapter.compile_rename_table_migration(RenameTableMigration {
        from: "Account".to_string(),
        to: "account".to_string(),
    });
    assert_eq!(statements.len(), 2, "SQLite requires an intermediate name for a case-only rename");
    for statement in statements {
        adapter.execute(&statement, &[]).await?;
    }

    let rows = adapter.execute("UPDATE account SET name = 'still-preserved' WHERE name = 'preserved'", &[]).await?;
    assert_eq!(rows, 1);
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
async fn sqlite_quotes_case_sensitive_identifiers_but_keeps_wildcards() -> anyhow::Result<()> {
    let adapter = SqliteAdapter::new(":memory:".to_string()).await.map_err(anyhow::Error::msg)?;
    let (select, _) = adapter.compile_find_query(FindQuery::new(&["*", "createdAt"], "AudioCreation", -1, -1));

    assert_eq!(select, "SELECT *, \"createdAt\" FROM \"AudioCreation\"");
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

fn auth_method_values() -> Vec<String> {
    vec!["PASSWORD".to_string(), "GOOGLE".to_string()]
}

fn auth_method_table() -> CreateTableMigration {
    CreateTableMigration {
        table: "account".to_string(),
        if_not_exists: false,
        columns: vec![MigrationColumn {
            name: "auth_method".to_string(),
            ty: MigrationColumnType::Enum { name: "AuthMethod".to_string(), values: auth_method_values() },
            primary_key: false,
            unique: false,
            nullable: false,
            default: None,
        }],
        foreign_keys: Vec::new(),
    }
}

#[tokio::test]
async fn sqlite_uses_text_with_a_check_constraint_for_enums() -> anyhow::Result<()> {
    let adapter = SqliteAdapter::new(":memory:".to_string()).await.map_err(anyhow::Error::msg)?;
    let sql = adapter.compile_create_table_migration(auth_method_table());

    assert!(sql.contains("auth_method TEXT CHECK (auth_method IN ('PASSWORD', 'GOOGLE')) NOT NULL"), "{sql}");

    Ok(())
}

#[tokio::test]
async fn postgres_uses_a_named_native_enum_type() -> anyhow::Result<()> {
    // The PgBouncer constructor builds its pool lazily, so compiling SQL needs no server.
    let adapter = PostgresAdapter::pgbouncer("postgres://postgres@localhost/compile_only").await?;

    assert_eq!(
        adapter.compile_create_enum_migration(CreateEnumMigration {
            name: "AuthMethod".to_string(),
            values: auth_method_values(),
        }),
        ["CREATE TYPE \"AuthMethod\" AS ENUM ('PASSWORD', 'GOOGLE');"]
    );
    let sql = adapter.compile_create_table_migration(auth_method_table());
    assert!(sql.contains("auth_method \"AuthMethod\" NOT NULL"), "{sql}");

    Ok(())
}

#[tokio::test]
async fn mysql_uses_an_inline_native_enum() {
    let adapter = MySqlAdapter::new("mysql://root@localhost/compile_only");
    let sql = adapter.compile_create_table_migration(auth_method_table());

    assert!(sql.contains("auth_method ENUM('PASSWORD', 'GOOGLE') NOT NULL"), "{sql}");
}
