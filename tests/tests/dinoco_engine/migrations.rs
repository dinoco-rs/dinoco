use dinoco_engine::{
    CreateIndexMigration, CreateTableMigration, DinocoAdapter, DinocoSqlCompiler, DropIndexMigration, MigrationColumn,
    MigrationColumnType, MigrationDefault, MigrationIndex, MigrationIndexKind, MySqlAdapter, PostgresAdapter,
    SqliteAdapter,
};

fn migration() -> CreateTableMigration {
    CreateTableMigration {
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
                name: "created_at".to_string(),
                ty: MigrationColumnType::DateTime,
                primary_key: false,
                unique: false,
                nullable: false,
                default: Some(MigrationDefault::CurrentTimestamp),
            },
        ],
        foreign_keys: Vec::new(),
    }
}

#[tokio::test]
async fn adapters_compile_migration_sql_with_their_dialect() -> anyhow::Result<()> {
    let sqlite = SqliteAdapter::new(":memory:".to_string()).await.map_err(anyhow::Error::msg)?;
    let sqlite_sql = sqlite.compile_create_table_migration(migration());
    assert!(sqlite_sql.contains("id TEXT PRIMARY KEY NOT NULL"));
    assert!(sqlite_sql.contains("DEFAULT CURRENT_TIMESTAMP"));

    let mysql = MySqlAdapter::new("mysql://root:root@localhost:3306/mysql");
    let mysql_sql = mysql.compile_create_table_migration(migration());
    assert!(mysql_sql.contains("CREATE TABLE IF NOT EXISTS account"));
    assert!(mysql_sql.contains("id VARCHAR(255) PRIMARY KEY NOT NULL"));

    assert!(sqlite.compile_create_migrations_table().contains("dinoco_migrations"));
    assert!(mysql.compile_insert_migration_record("001_initial").contains("001_initial"));

    Ok(())
}

#[tokio::test]
async fn adapters_compile_schema_types_with_expected_database_types() -> anyhow::Result<()> {
    let columns = vec![
        MigrationColumn {
            name: "string_value".to_string(),
            ty: MigrationColumnType::String,
            primary_key: false,
            unique: false,
            nullable: false,
            default: None,
        },
        MigrationColumn {
            name: "boolean_value".to_string(),
            ty: MigrationColumnType::Boolean,
            primary_key: false,
            unique: false,
            nullable: false,
            default: None,
        },
        MigrationColumn {
            name: "integer_value".to_string(),
            ty: MigrationColumnType::Integer,
            primary_key: false,
            unique: false,
            nullable: false,
            default: None,
        },
        MigrationColumn {
            name: "float_value".to_string(),
            ty: MigrationColumnType::Float,
            primary_key: false,
            unique: false,
            nullable: false,
            default: None,
        },
        MigrationColumn {
            name: "datetime_value".to_string(),
            ty: MigrationColumnType::DateTime,
            primary_key: false,
            unique: false,
            nullable: false,
            default: None,
        },
        MigrationColumn {
            name: "date_value".to_string(),
            ty: MigrationColumnType::Date,
            primary_key: false,
            unique: false,
            nullable: false,
            default: None,
        },
        MigrationColumn {
            name: "json_value".to_string(),
            ty: MigrationColumnType::Json,
            primary_key: false,
            unique: false,
            nullable: false,
            default: None,
        },
        MigrationColumn {
            name: "enum_value".to_string(),
            ty: MigrationColumnType::Enum {
                name: "OfficeType".to_string(),
                values: vec!["admin".to_string(), "member".to_string()],
            },
            primary_key: false,
            unique: false,
            nullable: false,
            default: None,
        },
    ];
    let migration = CreateTableMigration {
        table: "type_matrix".to_string(),
        columns,
        foreign_keys: Vec::new(),
        if_not_exists: false,
    };

    let sqlite = SqliteAdapter::new(":memory:".to_string()).await.map_err(anyhow::Error::msg)?;
    let sqlite_sql = sqlite.compile_create_table_migration(migration.clone());
    assert!(sqlite_sql.contains("string_value TEXT"));
    assert!(sqlite_sql.contains("boolean_value BOOLEAN"));
    assert!(sqlite_sql.contains("integer_value INTEGER"));
    assert!(sqlite_sql.contains("float_value REAL"));
    assert!(sqlite_sql.contains("datetime_value DATETIME"));
    assert!(sqlite_sql.contains("date_value DATE"));
    assert!(sqlite_sql.contains("json_value BLOB"));
    assert!(sqlite_sql.contains("enum_value TEXT CHECK (enum_value IN ('admin', 'member'))"));

    let postgres = PostgresAdapter::pgbouncer("postgres://postgres:postgres@localhost:5432/postgres").await?;
    let postgres_sql = postgres.compile_create_table_migration(migration.clone());
    assert!(postgres_sql.contains("string_value TEXT"));
    assert!(postgres_sql.contains("boolean_value BOOLEAN"));
    assert!(postgres_sql.contains("integer_value BIGINT"));
    assert!(postgres_sql.contains("float_value DOUBLE PRECISION"));
    assert!(postgres_sql.contains("datetime_value TIMESTAMP"));
    assert!(postgres_sql.contains("date_value DATE"));
    assert!(postgres_sql.contains("json_value JSONB"));
    assert!(postgres_sql.contains("enum_value \"OfficeType\""));
    assert_eq!(
        postgres.compile_create_enum_migration(dinoco_engine::CreateEnumMigration {
            name: "OfficeType".to_string(),
            values: vec!["admin".to_string(), "member".to_string()],
        }),
        ["CREATE TYPE \"OfficeType\" AS ENUM ('admin', 'member');"]
    );

    let mysql = MySqlAdapter::new("mysql://root:root@localhost:3306/mysql");
    let mysql_sql = mysql.compile_create_table_migration(migration);
    assert!(mysql_sql.contains("string_value VARCHAR(255)"));
    assert!(mysql_sql.contains("boolean_value TINYINT(1)"));
    assert!(mysql_sql.contains("integer_value BIGINT"));
    assert!(mysql_sql.contains("float_value DOUBLE PRECISION"));
    assert!(mysql_sql.contains("datetime_value TIMESTAMP"));
    assert!(mysql_sql.contains("date_value DATE"));
    assert!(mysql_sql.contains("json_value JSON"));
    assert!(mysql_sql.contains("enum_value ENUM('admin', 'member')"));

    Ok(())
}

#[tokio::test]
async fn adapters_compile_index_migrations_with_their_dialect() -> anyhow::Result<()> {
    let index = MigrationIndex {
        name: "idx_post_tenant_author".to_string(),
        columns: vec!["tenant_id".to_string(), "author_id".to_string()],
        automatic: false,
        kind: MigrationIndexKind::Standard,
        predicate: None,
    };
    let create = CreateIndexMigration { table: "post".to_string(), index: index.clone() };
    let drop = DropIndexMigration { table: "post".to_string(), index };

    let sqlite = SqliteAdapter::new(":memory:".to_string()).await.map_err(anyhow::Error::msg)?;
    assert_eq!(
        sqlite.compile_create_index_migration(create.clone()),
        "CREATE INDEX idx_post_tenant_author ON post (tenant_id, author_id);"
    );
    assert_eq!(sqlite.compile_drop_index_migration(drop.clone()), "DROP INDEX idx_post_tenant_author;");

    let postgres = PostgresAdapter::pgbouncer("postgres://postgres:postgres@localhost:5432/postgres").await?;
    assert_eq!(
        postgres.compile_create_index_migration(create.clone()),
        "CREATE INDEX idx_post_tenant_author ON post (tenant_id, author_id);"
    );
    assert_eq!(postgres.compile_drop_index_migration(drop.clone()), "DROP INDEX idx_post_tenant_author;");

    let mysql = MySqlAdapter::new("mysql://root:root@localhost:3306/mysql");
    assert_eq!(
        mysql.compile_create_index_migration(create),
        "CREATE INDEX idx_post_tenant_author ON post (tenant_id, author_id);"
    );
    assert_eq!(mysql.compile_drop_index_migration(drop), "DROP INDEX idx_post_tenant_author ON post;");

    let fulltext = CreateIndexMigration {
        table: "post".to_string(),
        index: MigrationIndex {
            name: "idx_post_body_fulltext".to_string(),
            columns: vec!["body".to_string()],
            automatic: false,
            kind: MigrationIndexKind::FullText,
            predicate: None,
        },
    };
    assert!(sqlite.compile_create_index_migration(fulltext.clone()).contains("LIKE fallback"));
    assert_eq!(
        postgres.compile_create_index_migration(fulltext.clone()),
        "CREATE INDEX idx_post_body_fulltext ON post USING GIN (to_tsvector('simple', COALESCE(body, '')));"
    );
    assert_eq!(
        mysql.compile_create_index_migration(fulltext),
        "CREATE FULLTEXT INDEX idx_post_body_fulltext ON post (body);"
    );

    let unique = CreateIndexMigration {
        table: "post".to_string(),
        index: MigrationIndex {
            name: "uq_post_tenant_slug".to_string(),
            columns: vec!["tenant_id".to_string(), "slug".to_string()],
            automatic: false,
            kind: MigrationIndexKind::Unique,
            predicate: None,
        },
    };
    assert_eq!(
        sqlite.compile_create_index_migration(unique.clone()),
        "CREATE UNIQUE INDEX uq_post_tenant_slug ON post (tenant_id, slug);"
    );
    assert_eq!(
        postgres.compile_create_index_migration(unique.clone()),
        "CREATE UNIQUE INDEX uq_post_tenant_slug ON post (tenant_id, slug);"
    );
    assert_eq!(
        mysql.compile_create_index_migration(unique),
        "CREATE UNIQUE INDEX uq_post_tenant_slug ON post (tenant_id, slug);"
    );

    let composite_fulltext = CreateIndexMigration {
        table: "post".to_string(),
        index: MigrationIndex {
            name: "idx_post_title_body_fulltext".to_string(),
            columns: vec!["title".to_string(), "body".to_string()],
            automatic: false,
            kind: MigrationIndexKind::FullText,
            predicate: None,
        },
    };
    assert_eq!(
        postgres.compile_create_index_migration(composite_fulltext.clone()),
        "CREATE INDEX idx_post_title_body_fulltext ON post USING GIN (to_tsvector('simple', COALESCE(title, '') || ' ' || COALESCE(body, '')));"
    );
    assert_eq!(
        mysql.compile_create_index_migration(composite_fulltext),
        "CREATE FULLTEXT INDEX idx_post_title_body_fulltext ON post (title, body);"
    );

    Ok(())
}

#[tokio::test]
async fn adapters_compile_partial_indexes_and_keep_the_condition_in_a_comment() -> anyhow::Result<()> {
    use dinoco_engine::{IndexPredicate, PredicateOperator, PredicateValue};

    let condition = IndexPredicate::And(vec![
        IndexPredicate::Compare {
            field: "deleted_at".to_string(),
            operator: PredicateOperator::Eq,
            value: PredicateValue::Null,
        },
        IndexPredicate::Compare {
            field: "slug".to_string(),
            operator: PredicateOperator::Neq,
            value: PredicateValue::String("it's \\".to_string()),
        },
    ]);
    let index = |kind, name: &str| CreateIndexMigration {
        table: "post".to_string(),
        index: MigrationIndex {
            name: name.to_string(),
            columns: vec!["tenant_id".to_string(), "slug".to_string()],
            automatic: false,
            kind,
            predicate: Some(condition.clone()),
        },
    };
    let unique = index(MigrationIndexKind::Unique, "uq_post_tenant_slug");
    let standard = index(MigrationIndexKind::Standard, "idx_post_tenant_slug");
    let comment = r#"dinoco:where deleted_at == null && slug != "it's \\""#;

    let sqlite = SqliteAdapter::new(":memory:".to_string()).await.map_err(anyhow::Error::msg)?;
    assert_eq!(
        sqlite.compile_create_index_migration(unique.clone()),
        format!(
            "CREATE UNIQUE INDEX uq_post_tenant_slug ON post (tenant_id, slug) WHERE deleted_at IS NULL AND slug <> \
             'it''s \\' /* {comment} */;"
        )
    );

    let postgres = PostgresAdapter::pgbouncer("postgres://postgres:postgres@localhost:5432/postgres").await?;
    assert_eq!(
        postgres.compile_create_index_migration(standard.clone()),
        format!(
            "CREATE INDEX idx_post_tenant_slug ON post (tenant_id, slug) WHERE deleted_at IS NULL AND slug <> 'it''s \\';\n\
             COMMENT ON INDEX idx_post_tenant_slug IS '{}';",
            comment.replace('\'', "''")
        )
    );

    // MySQL has no partial indexes: a unique one keys every column by the
    // condition, a standard one covers every row.
    let mysql = MySqlAdapter::new("mysql://root:root@localhost:3306/mysql");
    let mysql_condition = "deleted_at IS NULL AND slug <> 'it''s \\\\'";
    let mysql_comment = comment.replace('\\', "\\\\").replace('\'', "''");
    assert_eq!(
        mysql.compile_create_index_migration(unique),
        format!(
            "CREATE UNIQUE INDEX uq_post_tenant_slug ON post ((CASE WHEN {mysql_condition} THEN tenant_id END), \
             (CASE WHEN {mysql_condition} THEN slug END)) COMMENT '{mysql_comment}';"
        )
    );
    assert_eq!(
        mysql.compile_create_index_migration(standard),
        format!("CREATE INDEX idx_post_tenant_slug ON post (tenant_id, slug) COMMENT '{mysql_comment}';")
    );

    // Reserved names are quoted inside the condition too.
    let reserved = CreateIndexMigration {
        table: "order".to_string(),
        index: MigrationIndex {
            name: "uq_order_key".to_string(),
            columns: vec!["key".to_string()],
            automatic: false,
            kind: MigrationIndexKind::Unique,
            predicate: Some(IndexPredicate::Field("desc".to_string())),
        },
    };
    assert!(sqlite.compile_create_index_migration(reserved.clone()).contains(r#"("key") WHERE "desc" = TRUE"#));
    assert!(mysql.compile_create_index_migration(reserved).contains("(CASE WHEN `desc` = TRUE THEN `key` END)"));

    Ok(())
}
