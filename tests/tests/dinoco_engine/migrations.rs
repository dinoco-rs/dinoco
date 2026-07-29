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
    assert!(sqlite_sql.contains("enum_value TEXT"));

    let postgres = PostgresAdapter::direct("postgres://postgres:postgres@localhost:5432/postgres").await?;
    let postgres_sql = postgres.compile_create_table_migration(migration.clone());
    assert!(postgres_sql.contains("string_value TEXT"));
    assert!(postgres_sql.contains("boolean_value BOOLEAN"));
    assert!(postgres_sql.contains("integer_value BIGINT"));
    assert!(postgres_sql.contains("float_value DOUBLE PRECISION"));
    assert!(postgres_sql.contains("datetime_value TIMESTAMP"));
    assert!(postgres_sql.contains("date_value DATE"));
    assert!(postgres_sql.contains("json_value JSONB"));
    assert!(postgres_sql.contains("enum_value OfficeType"));

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
    };
    let create = CreateIndexMigration { table: "post".to_string(), index: index.clone() };
    let drop = DropIndexMigration { table: "post".to_string(), index };

    let sqlite = SqliteAdapter::new(":memory:".to_string()).await.map_err(anyhow::Error::msg)?;
    assert_eq!(
        sqlite.compile_create_index_migration(create.clone()),
        "CREATE INDEX idx_post_tenant_author ON post (tenant_id, author_id);"
    );
    assert_eq!(sqlite.compile_drop_index_migration(drop.clone()), "DROP INDEX idx_post_tenant_author;");

    let postgres = PostgresAdapter::direct("postgres://postgres:postgres@localhost:5432/postgres").await?;
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
