use dinoco::{
    Entity, EntityExtend, count, find_first, find_many, insert_into, insert_many, transaction, transactions, update,
};
use dinoco_engine::{
    Backend, CreateIndexMigration, CreateTableMigration, DinocoAdapter, DinocoClient, DinocoSqlCompiler,
    MigrationColumn, MigrationColumnType, MigrationDefault, MigrationForeignKey, MigrationIndex, MigrationIndexKind,
    MySqlAdapter, PostgresAdapter, ReferentialAction, SqliteAdapter,
};
use dinoco_tests::{column, create_table, default, drop_table, nullable, primary};

const POSTGRES_URL: &str = "postgres://postgres:postgres@localhost:5432/postgres";
const MYSQL_URL: &str = "mysql://root:root@localhost:3306/mysql";
static POSTGRES_TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
static MYSQL_TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[derive(Debug, Entity)]
#[dinoco(table_name = "all_methods_user")]
pub struct User {
    #[dinoco(auto_generate = uuid)]
    id: String,
    email: String,
    office: String,
    age: i64,

    #[dinoco(one_to_many, foreign_key = "user_id", references = "id")]
    tokens: Vec<Token>,
}

#[derive(Debug, Entity)]
#[dinoco(table_name = "all_methods_token")]
pub struct Token {
    #[dinoco(auto_generate = uuid)]
    id: String,
    #[dinoco(default = false)]
    is_expired: bool,
    user_id: Option<String>,

    #[dinoco(one_to_many, foreign_key = "user_id", references = "id")]
    user: Option<User>,
}

#[derive(Debug, Entity)]
#[dinoco(table_name = "all_methods_user_post")]
pub struct UserPost {
    user_id: String,
    post_id: String,
}

#[derive(Debug, Entity)]
#[dinoco(table_name = "adapter_transaction_account")]
pub struct TransactionAccount {
    id: String,
    email: String,
}

#[derive(Debug, Entity)]
#[dinoco(table_name = "adapter_transaction_business")]
pub struct AdapterTransactionBusiness {
    #[dinoco(primary_key)]
    id: String,
    name: String,

    #[dinoco(
        many_to_many,
        foreign_key = "id",
        references = "id",
        join_table = "_adapter_transaction_business_to_system",
        parent_field = "id",
        join_parent_field = "business_id",
        join_child_field = "system_id"
    )]
    systems: Vec<AdapterTransactionSystem>,

    #[dinoco(
        many_to_many_key,
        join_table = "_adapter_transaction_business_to_system",
        parent_field = "id",
        join_parent_field = "business_id",
        join_child_field = "system_id"
    )]
    system_id: Option<String>,
}

#[derive(Debug, Entity)]
#[dinoco(table_name = "adapter_transaction_system")]
pub struct AdapterTransactionSystem {
    #[dinoco(primary_key)]
    id: String,
    name: String,

    #[dinoco(
        many_to_many,
        foreign_key = "id",
        references = "id",
        join_table = "_adapter_transaction_business_to_system",
        parent_field = "id",
        join_parent_field = "system_id",
        join_child_field = "business_id"
    )]
    businesses: Vec<AdapterTransactionBusiness>,

    #[dinoco(
        many_to_many_key,
        join_table = "_adapter_transaction_business_to_system",
        parent_field = "id",
        join_parent_field = "system_id",
        join_child_field = "business_id"
    )]
    business_id: Option<String>,
}

#[derive(Debug, Entity)]
#[dinoco(table_name = "adapter_fulltext_document")]
pub struct FullTextDocument {
    id: String,
    #[dinoco(fulltext = "body,title")]
    title: String,
    #[dinoco(fulltext = "body,title")]
    body: String,
}

#[derive(Debug, Entity)]
#[dinoco(table_name = "adapter_postgres_temporal")]
pub struct PostgresTemporalRecord {
    #[dinoco(primary_key)]
    id: String,
    verified_at: dinoco::chrono::DateTime<dinoco::chrono::Utc>,
    verification_day: dinoco::chrono::NaiveDate,
    payload: dinoco::serde_json::Value,
}

#[derive(Debug, EntityExtend)]
#[extend(User)]
pub struct UserSelect {
    id: String,
    email: String,
}

#[tokio::test]
async fn sqlite_adapter_runs_all_dinoco_methods() -> anyhow::Result<()> {
    let path = format!("/private/tmp/dinoco-all-methods-{}-{}.sqlite", std::process::id(), monotonic());
    let adapter = SqliteAdapter::new(path.clone()).await.map_err(anyhow::Error::msg)?;
    reset_schema(&adapter).await?;
    run_all_methods(DinocoClient::new(Backend::Sqlite(adapter))).await?;
    let _ = std::fs::remove_file(path);
    Ok(())
}

#[tokio::test]
async fn postgres_adapter_runs_all_dinoco_methods() -> anyhow::Result<()> {
    let _guard = POSTGRES_TEST_LOCK.lock().await;
    let adapter = PostgresAdapter::direct(POSTGRES_URL).await?;
    reset_schema(&adapter).await?;
    run_all_methods(DinocoClient::new(Backend::Postgres(adapter))).await
}

#[tokio::test]
async fn postgres_direct_applies_configured_pool_limits() -> anyhow::Result<()> {
    let _guard = POSTGRES_TEST_LOCK.lock().await;
    let adapter = PostgresAdapter::direct_with_pool(POSTGRES_URL, 3, 7).await?;
    let status = adapter.pool.status();

    assert_eq!(status.max_size, 7);
    assert_eq!(status.size, 3);
    assert_eq!(status.available, 3);
    Ok(())
}

#[tokio::test]
#[allow(clippy::needless_borrows_for_generic_args)]
async fn postgres_serializes_and_decodes_utc_datetime_for_timestamp_columns() -> anyhow::Result<()> {
    let _guard = POSTGRES_TEST_LOCK.lock().await;
    let adapter = PostgresAdapter::direct(POSTGRES_URL).await?;
    drop_table(&adapter, "adapter_postgres_temporal").await?;
    create_table(
        &adapter,
        "adapter_postgres_temporal",
        vec![
            primary(column("id", MigrationColumnType::String)),
            column("verified_at", MigrationColumnType::DateTime),
            column("verification_day", MigrationColumnType::Date),
            column("payload", MigrationColumnType::Json),
        ],
    )
    .await?;
    let client = DinocoClient::new(Backend::Postgres(adapter));
    let initial = dinoco::chrono::DateTime::from_timestamp(1_700_000_000, 123_456_000).expect("valid timestamp");
    let updated = dinoco::chrono::DateTime::from_timestamp(1_700_003_600, 654_321_000).expect("valid timestamp");
    let initial_payload = dinoco::serde_json::json!({ "verified": false });
    let updated_payload = dinoco::serde_json::json!({ "verified": true });

    dinoco::insert_into::<PostgresTemporalRecord>()
        .values(PostgresTemporalRecord {
            id: "verification-1".to_string(),
            verified_at: initial,
            verification_day: initial.date_naive(),
            payload: initial_payload.clone(),
        })
        .execute(&client)
        .await?;

    let inserted = find_first::<PostgresTemporalRecord>()
        .where_(|item| item.verified_at.eq(initial))
        .where_(|item| item.verification_day.eq(initial.date_naive()))
        .where_(|item| item.payload.eq(initial_payload))
        .execute(&client)
        .await?
        .expect("inserted temporal record");
    assert_eq!(inserted.verified_at, initial);

    let changed = dinoco::find_and_update::<PostgresTemporalRecord>()
        .where_(|item| item.verified_at.eq(&initial))
        .update(|item| item.verified_at.set(updated))
        .update(|item| item.verification_day.set(updated.date_naive()))
        .update(|item| item.payload.set(&updated_payload))
        .execute(&client)
        .await?;
    assert_eq!(changed.verified_at, updated);
    assert_eq!(changed.verification_day, updated.date_naive());
    assert_eq!(changed.payload, updated_payload);

    Ok(())
}

#[tokio::test]
async fn mysql_adapter_runs_all_dinoco_methods() -> anyhow::Result<()> {
    let _guard = MYSQL_TEST_LOCK.lock().await;
    let adapter = MySqlAdapter::new(MYSQL_URL);
    reset_schema(&adapter).await?;
    run_all_methods(DinocoClient::new(Backend::Mysql(adapter))).await
}

#[tokio::test]
async fn sqlite_adapter_commits_and_rolls_back_transaction_batches() -> anyhow::Result<()> {
    let path = format!("/private/tmp/dinoco-adapter-transactions-{}-{}.sqlite", std::process::id(), monotonic());
    let adapter = SqliteAdapter::new(path.clone()).await.map_err(anyhow::Error::msg)?;
    reset_transaction_schema(&adapter).await?;
    run_transactions(DinocoClient::new(Backend::Sqlite(adapter))).await?;
    let _ = std::fs::remove_file(path);
    Ok(())
}

#[tokio::test]
async fn postgres_adapter_commits_and_rolls_back_transaction_batches() -> anyhow::Result<()> {
    let _guard = POSTGRES_TEST_LOCK.lock().await;
    let adapter = PostgresAdapter::direct(POSTGRES_URL).await?;
    reset_transaction_schema(&adapter).await?;
    run_transactions(DinocoClient::new(Backend::Postgres(adapter))).await
}

#[tokio::test]
async fn mysql_adapter_commits_and_rolls_back_transaction_batches() -> anyhow::Result<()> {
    let _guard = MYSQL_TEST_LOCK.lock().await;
    let adapter = MySqlAdapter::new(MYSQL_URL);
    reset_transaction_schema(&adapter).await?;
    run_transactions(DinocoClient::new(Backend::Mysql(adapter))).await
}

#[tokio::test]
async fn postgres_adapter_creates_introspects_and_queries_fulltext_indexes() -> anyhow::Result<()> {
    let _guard = POSTGRES_TEST_LOCK.lock().await;
    let adapter = PostgresAdapter::direct(POSTGRES_URL).await?;
    reset_fulltext_schema(&adapter).await?;
    assert_fulltext_index(dinoco_cli::db::CliDatabase::Postgres(PostgresAdapter::direct(POSTGRES_URL).await?)).await?;
    run_fulltext_query(DinocoClient::new(Backend::Postgres(adapter))).await
}

#[tokio::test]
async fn mysql_adapter_creates_introspects_and_queries_fulltext_indexes() -> anyhow::Result<()> {
    let _guard = MYSQL_TEST_LOCK.lock().await;
    let adapter = MySqlAdapter::new(MYSQL_URL);
    reset_fulltext_schema(&adapter).await?;
    assert_fulltext_index(dinoco_cli::db::CliDatabase::Mysql(MySqlAdapter::new(MYSQL_URL))).await?;
    run_fulltext_query(DinocoClient::new(Backend::Mysql(adapter))).await
}

#[tokio::test]
async fn sqlite_adapter_detects_migration_changes_from_introspection() -> anyhow::Result<()> {
    let path = format!("/private/tmp/dinoco-detect-{}-{}.sqlite", std::process::id(), monotonic());
    let adapter = SqliteAdapter::new(path.clone()).await.map_err(anyhow::Error::msg)?;
    reset_detection_schema(&adapter).await?;
    seed_detection_schema(&adapter).await?;
    assert_detection_plan(dinoco_cli::db::CliDatabase::Sqlite(adapter)).await?;
    let _ = std::fs::remove_file(path);
    Ok(())
}

#[tokio::test]
async fn postgres_adapter_detects_migration_changes_from_introspection() -> anyhow::Result<()> {
    let _guard = POSTGRES_TEST_LOCK.lock().await;
    let adapter = PostgresAdapter::direct(POSTGRES_URL).await?;
    reset_detection_schema(&adapter).await?;
    seed_detection_schema(&adapter).await?;
    assert_detection_plan(dinoco_cli::db::CliDatabase::Postgres(adapter)).await
}

#[tokio::test]
async fn mysql_adapter_detects_migration_changes_from_introspection() -> anyhow::Result<()> {
    let _guard = MYSQL_TEST_LOCK.lock().await;
    let adapter = MySqlAdapter::new(MYSQL_URL);
    reset_detection_schema(&adapter).await?;
    seed_detection_schema(&adapter).await?;
    assert_detection_plan(dinoco_cli::db::CliDatabase::Mysql(adapter)).await
}

#[tokio::test]
async fn sqlite_preserves_unique_identity_boolean_and_composite_keys_in_introspection() -> anyhow::Result<()> {
    let path = format!("/private/tmp/dinoco-constraints-{}-{}.sqlite", std::process::id(), monotonic());
    let adapter = SqliteAdapter::new(path.clone()).await.map_err(anyhow::Error::msg)?;
    seed_constraint_schema(&adapter).await?;
    assert_constraint_introspection(dinoco_cli::db::CliDatabase::Sqlite(adapter)).await?;
    let _ = std::fs::remove_file(path);
    Ok(())
}

#[tokio::test]
async fn postgres_preserves_unique_identity_boolean_and_composite_keys_in_introspection() -> anyhow::Result<()> {
    let _guard = POSTGRES_TEST_LOCK.lock().await;
    let adapter = PostgresAdapter::direct(POSTGRES_URL).await?;
    seed_constraint_schema(&adapter).await?;
    assert_constraint_introspection(dinoco_cli::db::CliDatabase::Postgres(adapter)).await
}

#[tokio::test]
async fn mysql_preserves_unique_identity_boolean_and_composite_keys_in_introspection() -> anyhow::Result<()> {
    let _guard = MYSQL_TEST_LOCK.lock().await;
    let adapter = MySqlAdapter::new(MYSQL_URL);
    seed_constraint_schema(&adapter).await?;
    assert_constraint_introspection(dinoco_cli::db::CliDatabase::Mysql(adapter)).await
}

async fn reset_schema<A>(adapter: &A) -> anyhow::Result<()>
where
    A: DinocoAdapter + DinocoSqlCompiler,
{
    drop_table(adapter, "all_methods_user_post").await?;
    drop_table(adapter, "all_methods_token").await?;
    drop_table(adapter, "all_methods_user").await?;

    create_table(
        adapter,
        "all_methods_user",
        vec![
            primary(column("id", MigrationColumnType::String)),
            column("email", MigrationColumnType::String),
            column("office", MigrationColumnType::String),
            column("age", MigrationColumnType::Integer),
        ],
    )
    .await?;
    create_table(
        adapter,
        "all_methods_token",
        vec![
            primary(column("id", MigrationColumnType::String)),
            default(column("is_expired", MigrationColumnType::Boolean), MigrationDefault::Boolean(false)),
            nullable(column("user_id", MigrationColumnType::String)),
        ],
    )
    .await?;
    create_table(
        adapter,
        "all_methods_user_post",
        vec![column("user_id", MigrationColumnType::String), column("post_id", MigrationColumnType::String)],
    )
    .await?;

    Ok(())
}

async fn reset_transaction_schema<A>(adapter: &A) -> anyhow::Result<()>
where
    A: DinocoAdapter + DinocoSqlCompiler,
{
    drop_table(adapter, "_adapter_transaction_business_to_system").await?;
    drop_table(adapter, "adapter_transaction_system").await?;
    drop_table(adapter, "adapter_transaction_business").await?;
    drop_table(adapter, "adapter_transaction_account").await?;
    create_table(
        adapter,
        "adapter_transaction_account",
        vec![primary(column("id", MigrationColumnType::String)), column("email", MigrationColumnType::String)],
    )
    .await?;
    create_table(
        adapter,
        "adapter_transaction_business",
        vec![primary(column("id", MigrationColumnType::String)), column("name", MigrationColumnType::String)],
    )
    .await?;
    create_table(
        adapter,
        "adapter_transaction_system",
        vec![primary(column("id", MigrationColumnType::String)), column("name", MigrationColumnType::String)],
    )
    .await?;
    create_table(
        adapter,
        "_adapter_transaction_business_to_system",
        vec![
            primary(column("business_id", MigrationColumnType::String)),
            primary(column("system_id", MigrationColumnType::String)),
        ],
    )
    .await
}

async fn reset_detection_schema<A>(adapter: &A) -> anyhow::Result<()>
where
    A: DinocoAdapter + DinocoSqlCompiler,
{
    drop_table(adapter, "migration_detect_old_rel").await?;
    drop_table(adapter, "migration_detect_post").await?;
    drop_table(adapter, "migration_detect_user").await?;
    Ok(())
}

async fn seed_detection_schema<A>(adapter: &A) -> anyhow::Result<()>
where
    A: DinocoAdapter + DinocoSqlCompiler,
{
    let user_id = MigrationColumn {
        name: "id".to_string(),
        ty: MigrationColumnType::Integer,
        primary_key: true,
        unique: false,
        nullable: false,
        default: None,
    };
    adapter
        .execute(
            &adapter.compile_create_table_migration(CreateTableMigration {
                table: "migration_detect_user".to_string(),
                columns: vec![
                    user_id.clone(),
                    column("name", MigrationColumnType::String),
                    column("legacy", MigrationColumnType::Integer),
                ],
                foreign_keys: Vec::new(),
                if_not_exists: false,
            }),
            &[],
        )
        .await?;
    adapter
        .execute(
            &adapter.compile_create_table_migration(CreateTableMigration {
                table: "migration_detect_post".to_string(),
                columns: vec![user_id.clone(), nullable(column("user_id", MigrationColumnType::Integer))],
                foreign_keys: Vec::new(),
                if_not_exists: false,
            }),
            &[],
        )
        .await?;
    adapter
        .execute(
            &adapter.compile_create_table_migration(CreateTableMigration {
                table: "migration_detect_old_rel".to_string(),
                columns: vec![user_id, column("user_id", MigrationColumnType::Integer)],
                foreign_keys: vec![MigrationForeignKey {
                    name: "fk_migration_detect_old_rel_user_id".to_string(),
                    columns: vec!["user_id".to_string()],
                    references_table: "migration_detect_user".to_string(),
                    references_columns: vec!["id".to_string()],
                    on_update: ReferentialAction::NoAction,
                    on_delete: ReferentialAction::NoAction,
                }],
                if_not_exists: false,
            }),
            &[],
        )
        .await?;

    adapter.execute("INSERT INTO migration_detect_user (id, name, legacy) VALUES (1, 'old', 10)", &[]).await?;
    Ok(())
}

async fn seed_constraint_schema<A>(adapter: &A) -> anyhow::Result<()>
where
    A: DinocoAdapter + DinocoSqlCompiler,
{
    drop_table(adapter, "migration_constraints_join").await?;
    drop_table(adapter, "migration_constraints_child").await?;
    drop_table(adapter, "migration_constraints_parent").await?;
    drop_table(adapter, "migration_constraints").await?;
    for statement in adapter
        .compile_drop_enum_migration(dinoco_engine::DropEnumMigration { name: "migration_constraint_role".to_string() })
    {
        adapter.execute(&statement, &[]).await?;
    }
    for statement in adapter.compile_create_enum_migration(dinoco_engine::CreateEnumMigration {
        name: "migration_constraint_role".to_string(),
        values: vec!["member".to_string(), "admin".to_string()],
    }) {
        adapter.execute(&statement, &[]).await?;
    }

    adapter
        .execute(
            &adapter.compile_create_table_migration(CreateTableMigration {
                table: "migration_constraints".to_string(),
                columns: vec![
                    MigrationColumn {
                        name: "id".to_string(),
                        ty: MigrationColumnType::Integer,
                        primary_key: true,
                        unique: false,
                        nullable: false,
                        default: Some(MigrationDefault::AutoIncrement),
                    },
                    MigrationColumn {
                        name: "owner_id".to_string(),
                        ty: MigrationColumnType::Integer,
                        primary_key: false,
                        unique: true,
                        nullable: false,
                        default: None,
                    },
                    MigrationColumn {
                        name: "active".to_string(),
                        ty: MigrationColumnType::Boolean,
                        primary_key: false,
                        unique: false,
                        nullable: false,
                        default: Some(MigrationDefault::Boolean(false)),
                    },
                    MigrationColumn {
                        name: "role".to_string(),
                        ty: MigrationColumnType::Enum {
                            name: "migration_constraint_role".to_string(),
                            values: vec!["member".to_string(), "admin".to_string()],
                        },
                        primary_key: false,
                        unique: false,
                        nullable: false,
                        default: Some(MigrationDefault::String("member".to_string())),
                    },
                ],
                foreign_keys: Vec::new(),
                if_not_exists: false,
            }),
            &[],
        )
        .await?;
    adapter
        .execute(
            &adapter.compile_create_table_migration(CreateTableMigration {
                table: "migration_constraints_join".to_string(),
                columns: vec![
                    MigrationColumn {
                        name: "left_id".to_string(),
                        ty: MigrationColumnType::Integer,
                        primary_key: true,
                        unique: false,
                        nullable: false,
                        default: None,
                    },
                    MigrationColumn {
                        name: "right_id".to_string(),
                        ty: MigrationColumnType::Integer,
                        primary_key: true,
                        unique: false,
                        nullable: false,
                        default: None,
                    },
                ],
                foreign_keys: Vec::new(),
                if_not_exists: false,
            }),
            &[],
        )
        .await?;
    adapter
        .execute(
            &adapter.compile_create_table_migration(CreateTableMigration {
                table: "migration_constraints_parent".to_string(),
                columns: vec![
                    MigrationColumn {
                        name: "tenant_id".to_string(),
                        ty: MigrationColumnType::Integer,
                        primary_key: true,
                        unique: false,
                        nullable: false,
                        default: None,
                    },
                    MigrationColumn {
                        name: "id".to_string(),
                        ty: MigrationColumnType::Integer,
                        primary_key: true,
                        unique: false,
                        nullable: false,
                        default: None,
                    },
                ],
                foreign_keys: Vec::new(),
                if_not_exists: false,
            }),
            &[],
        )
        .await?;
    adapter
        .execute(
            &adapter.compile_create_table_migration(CreateTableMigration {
                table: "migration_constraints_child".to_string(),
                columns: vec![
                    primary(column("id", MigrationColumnType::Integer)),
                    column("tenant_id", MigrationColumnType::Integer),
                    column("parent_id", MigrationColumnType::Integer),
                ],
                foreign_keys: vec![MigrationForeignKey {
                    name: "fk_migration_constraints_child_parent".to_string(),
                    columns: vec!["tenant_id".to_string(), "parent_id".to_string()],
                    references_table: "migration_constraints_parent".to_string(),
                    references_columns: vec!["tenant_id".to_string(), "id".to_string()],
                    on_update: ReferentialAction::Cascade,
                    on_delete: ReferentialAction::Cascade,
                }],
                if_not_exists: false,
            }),
            &[],
        )
        .await?;
    adapter
        .execute(
            &adapter.compile_create_index_migration(CreateIndexMigration {
                table: "migration_constraints_child".to_string(),
                index: MigrationIndex {
                    name: "idx_migration_constraints_child_parent".to_string(),
                    columns: vec!["tenant_id".to_string(), "parent_id".to_string()],
                    automatic: false,
                    kind: MigrationIndexKind::Standard,
                },
            }),
            &[],
        )
        .await?;
    adapter
        .execute(
            &adapter.compile_create_index_migration(CreateIndexMigration {
                table: "migration_constraints_child".to_string(),
                index: MigrationIndex {
                    name: "uq_migration_constraints_child_parent".to_string(),
                    columns: vec!["tenant_id".to_string(), "parent_id".to_string()],
                    automatic: false,
                    kind: MigrationIndexKind::Unique,
                },
            }),
            &[],
        )
        .await?;
    Ok(())
}

async fn assert_constraint_introspection(db: dinoco_cli::db::CliDatabase) -> anyhow::Result<()> {
    let is_sqlite = matches!(db, dinoco_cli::db::CliDatabase::Sqlite(_));
    let is_postgres =
        matches!(db, dinoco_cli::db::CliDatabase::Postgres(_) | dinoco_cli::db::CliDatabase::PgBouncer(_));
    let schema = db.inspect_schema().await?;
    let table = schema.tables.iter().find(|table| table.name == "migration_constraints").expect("constraint table");
    let id = table.columns.iter().find(|column| column.name == "id").expect("identity column");
    let owner = table.columns.iter().find(|column| column.name == "owner_id").expect("unique column");
    let active = table.columns.iter().find(|column| column.name == "active").expect("boolean column");
    assert!(id.primary_key);
    assert_eq!(id.default, Some(MigrationDefault::AutoIncrement));
    assert!(owner.unique);
    assert_eq!(active.ty, MigrationColumnType::Boolean);
    assert_eq!(active.default, Some(MigrationDefault::Boolean(false)));
    let role = table.columns.iter().find(|column| column.name == "role").expect("enum column");
    assert!(
        matches!(
            &role.ty,
            MigrationColumnType::Enum { name, values }
                if (!is_postgres || name == "migration_constraint_role")
                    && values == &["member".to_string(), "admin".to_string()]
        ),
        "{role:#?}"
    );

    let join =
        schema.tables.iter().find(|table| table.name == "migration_constraints_join").expect("composite-key table");
    assert_eq!(join.columns.iter().filter(|column| column.primary_key).count(), 2);
    let child =
        schema.tables.iter().find(|table| table.name == "migration_constraints_child").expect("composite-FK table");
    let foreign_key = child.foreign_keys.first().expect("composite foreign key");
    if !is_sqlite {
        assert_eq!(foreign_key.name, "fk_migration_constraints_child_parent");
    }
    assert_eq!(foreign_key.columns, ["tenant_id", "parent_id"]);
    assert_eq!(foreign_key.references_columns, ["tenant_id", "id"]);
    assert!(child.indexes.iter().any(|index| {
        index.name == "idx_migration_constraints_child_parent"
            && index.columns == ["tenant_id".to_string(), "parent_id".to_string()]
    }));
    assert!(child.indexes.iter().any(|index| {
        index.name == "uq_migration_constraints_child_parent"
            && index.columns == ["tenant_id".to_string(), "parent_id".to_string()]
            && index.kind == MigrationIndexKind::Unique
    }));
    Ok(())
}

async fn reset_fulltext_schema<A>(adapter: &A) -> anyhow::Result<()>
where
    A: DinocoAdapter + DinocoSqlCompiler,
{
    adapter
        .execute(
            &adapter.compile_drop_table_migration(dinoco_engine::DropTableMigration {
                table: "adapter_fulltext_document".to_string(),
                if_exists: true,
            }),
            &[],
        )
        .await?;
    adapter
        .execute(
            &adapter.compile_create_table_migration(CreateTableMigration {
                table: "adapter_fulltext_document".to_string(),
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
                        name: "title".to_string(),
                        ty: MigrationColumnType::String,
                        primary_key: false,
                        unique: false,
                        nullable: false,
                        default: None,
                    },
                    MigrationColumn {
                        name: "body".to_string(),
                        ty: MigrationColumnType::String,
                        primary_key: false,
                        unique: false,
                        nullable: false,
                        default: None,
                    },
                ],
                foreign_keys: Vec::new(),
                if_not_exists: false,
            }),
            &[],
        )
        .await?;
    adapter
        .execute(
            &adapter.compile_create_index_migration(CreateIndexMigration {
                table: "adapter_fulltext_document".to_string(),
                index: MigrationIndex {
                    name: "idx_adapter_fulltext_document_body_title_fulltext".to_string(),
                    columns: vec!["body".to_string(), "title".to_string()],
                    automatic: false,
                    kind: MigrationIndexKind::FullText,
                },
            }),
            &[],
        )
        .await?;
    Ok(())
}

async fn assert_fulltext_index(db: dinoco_cli::db::CliDatabase) -> anyhow::Result<()> {
    let schema = db.inspect_schema().await?;
    let table = schema.tables.iter().find(|table| table.name == "adapter_fulltext_document").expect("fulltext table");
    assert!(table.indexes.iter().any(|index| {
        index.name == "idx_adapter_fulltext_document_body_title_fulltext"
            && index.columns == ["body".to_string(), "title".to_string()]
            && index.kind == MigrationIndexKind::FullText
    }));
    Ok(())
}

async fn run_fulltext_query(client: DinocoClient) -> anyhow::Result<()> {
    insert_many::<FullTextDocument>()
        .values(vec![
            FullTextDocument::new(
                "document-1".to_string(),
                "Rust libraries".to_string(),
                "Dinoco makes Rust queries delightful".to_string(),
            ),
            FullTextDocument::new(
                "document-2".to_string(),
                "Database notes".to_string(),
                "A completely unrelated database record".to_string(),
            ),
        ])
        .execute(&client)
        .await?;

    let document = find_first::<FullTextDocument>()
        .where_(|x| x.title.fulltext("dinoco"))
        .execute(&client)
        .await?
        .expect("native full-text find_first");
    assert_eq!(document.id, "document-1");

    let documents = find_many::<FullTextDocument>()
        .where_complex(|x, m| m.and([x.title.fulltext("dinoco"), m.not(x.body.fulltext("unrelated"))]))
        .execute(&client)
        .await?;
    assert_eq!(documents.len(), 1);
    assert_eq!(documents[0].id, "document-1");

    let updated = dinoco::find_and_update::<FullTextDocument>()
        .where_(|x| x.body.fulltext("delightful"))
        .update(|x| x.body.set("Dinoco fulltext works across every find".to_string()))
        .execute(&client)
        .await?;
    assert_eq!(updated.id, "document-1");

    let documents = find_many::<FullTextDocument>().where_(|x| x.body.fulltext("fulltext")).execute(&client).await?;
    assert_eq!(documents.len(), 1);
    Ok(())
}

async fn assert_detection_plan(db: dinoco_cli::db::CliDatabase) -> anyhow::Result<()> {
    let mut current = db.inspect_schema().await?;
    current.tables.retain(|table| table.name.starts_with("migration_detect_"));
    current.enums.clear();

    let desired = dinoco_compiler::compile(
        r#"
        config {
            database = "postgresql"
            database_url = env("DATABASE_URL")
        }

        model MigrationDetectUser {
            id        Integer @id
            full_name String
            posts     MigrationDetectPost[]
        }

        model MigrationDetectPost {
            id      Integer @id
            user_id Integer?
            user    MigrationDetectUser? @relation(fields: [user_id], references: [id], onDelete: SetNull, onUpdate: Cascade)
        }

        model MigrationDetectOldRel {
            id      Integer @id
            user_id Integer
        }
        "#,
    )?;
    let plan = dinoco_cli::sql::plan_schema_migration(&desired, &current);

    assert!(
        plan.steps.iter().any(
            |step| matches!(step, dinoco_cli::sql::MigrationStep::RenameColumn(item) if item.table == "migration_detect_user" && item.from == "name" && item.to == "full_name")
        ),
        "plan did not detect rename: {plan:#?}"
    );
    assert!(
        plan.steps.iter().any(
            |step| matches!(step, dinoco_cli::sql::MigrationStep::DropColumn(item) if item.table == "migration_detect_user" && item.column == "legacy")
        ),
        "plan did not detect dropped column: {plan:#?}"
    );
    assert!(
        plan.steps.iter().any(
            |step| matches!(step, dinoco_cli::sql::MigrationStep::AddForeignKey(item) if item.table == "migration_detect_post" && item.foreign_key.on_delete == ReferentialAction::SetNull)
        ),
        "plan did not detect added relation: {plan:#?}"
    );
    assert!(
        plan.steps.iter().any(
            |step| matches!(step, dinoco_cli::sql::MigrationStep::DropForeignKey(item) if item.table == "migration_detect_old_rel")
        ),
        "plan did not detect removed relation: {plan:#?}"
    );
    assert!(
        plan.warnings.iter().any(|warning| warning.destructive && warning.message.contains("data will be lost")),
        "plan did not flag destructive data loss: {plan:#?}"
    );

    Ok(())
}

async fn run_all_methods(client: DinocoClient) -> anyhow::Result<()> {
    let mut user = User::new("a@dinoco.rs".to_string(), "admin".to_string(), 21);
    user.tokens = vec![Token::new(), Token::new()];
    insert_into::<User>().values(&user).execute(&client).await?;

    let returned = insert_into::<User>()
        .values(User::new("returning@dinoco.rs".to_string(), "admin".to_string(), 25))
        .returning::<UserSelect>()
        .execute(&client)
        .await?;
    assert_eq!(returned.email, "returning@dinoco.rs");

    let returned_many = insert_many::<User>()
        .values(vec![
            User::new("many-a@dinoco.rs".to_string(), "admin".to_string(), 30),
            User::new("many-b@dinoco.rs".to_string(), "admin".to_string(), 40),
        ])
        .returning::<UserSelect>()
        .execute(&client)
        .await?;
    assert_eq!(returned_many.len(), 2);

    let users = find_many::<User>()
        .includes(|x| x.tokens().order_by(|token| token.id.asc()).take(10).skip(0))
        .order_by(|x| x.email.asc())
        .read_in_primary()
        .execute(&client)
        .await?;
    assert_eq!(users.iter().find(|item| item.email == "a@dinoco.rs").expect("nested user").tokens.len(), 2);

    assert_eq!(find_many::<User>().where_(|x| x.email.like("dinoco")).execute(&client).await?.len(), 4);
    assert_eq!(find_many::<User>().where_(|x| x.email.starts_with("many")).execute(&client).await?.len(), 2);
    assert_eq!(
        find_many::<User>()
            .where_(|x| x.email.eq("ignored-before@dinoco.rs"))
            .where_complex(|x, m| {
                m.or(
                    m.and([x.email.eq("a@dinoco.rs"), x.age.eq(21)]),
                    m.or(
                        m.and([x.email.eq("many-a@dinoco.rs"), x.office.eq("admin")]),
                        m.and([x.email.eq("many-b@dinoco.rs"), m.not(x.age.lt(0))]),
                    ),
                )
            })
            .where_(|x| x.email.eq("ignored-after@dinoco.rs"))
            .execute(&client)
            .await?
            .len(),
        3
    );
    assert_eq!(
        find_first::<User>().where_(|x| x.email.ends_with("@dinoco.rs")).execute(&client).await?.unwrap().age,
        21
    );
    assert_eq!(find_many::<User>().where_(|x| x.age.between(20, 30)).execute(&client).await?.len(), 3);

    let selected = find_first::<User>()
        .select::<UserSelect>()
        .where_(|x| x.email.eq("a@dinoco.rs"))
        .read_in_primary()
        .execute(&client)
        .await?
        .expect("selected user");
    assert_eq!(selected.email, "a@dinoco.rs");

    let token = find_first::<Token>()
        .includes(|x| x.user())
        .where_(|x| x.user_id.not_null())
        .execute(&client)
        .await?
        .expect("token");
    assert!(token.user.is_some());

    dinoco::update::<User>()
        .where_(|x| x.email.eq("a@dinoco.rs"))
        .update(|x| x.email.set("b@dinoco.rs".to_string()))
        .execute(&client)
        .await?;

    let updated_rows = dinoco::update::<User>()
        .where_(|x| x.email.eq("b@dinoco.rs"))
        .update(|x| x.office.set("member".to_string()))
        .returning::<UserSelect>()
        .execute(&client)
        .await?;
    assert_eq!(updated_rows.len(), 1);

    let updated = dinoco::find_and_update::<User>()
        .where_(|x| x.email.eq("b@dinoco.rs"))
        .update(|x| x.office.set("owner".to_string()))
        .execute(&client)
        .await?;
    assert_eq!(updated.office, "owner");

    dinoco::update_many::<User>()
        .where_(|x| x.office.eq("admin"))
        .update(|x| x.office.set("staff".to_string()))
        .execute(&client)
        .await?;
    let staff = dinoco::update_many::<User>()
        .where_(|x| x.office.eq("staff"))
        .update(|x| x.office.set("manager".to_string()))
        .returning::<UserSelect>()
        .execute(&client)
        .await?;
    assert_eq!(staff.len(), 3);

    dinoco::update::<UserPost>()
        .where_(|x| x.user_id.eq("user-a"))
        .update(|x| x.post_id.connect("post-a"))
        .execute(&client)
        .await?;
    assert_eq!(dinoco::count::<UserPost>().execute(&client).await?.total, 1);
    dinoco::update_many::<UserPost>()
        .where_(|x| x.user_id.eq("user-a"))
        .update(|x| x.post_id.disconnect("post-a"))
        .execute(&client)
        .await?;
    assert_eq!(dinoco::count::<UserPost>().execute(&client).await?.total, 0);

    let count = dinoco::count::<User>()
        .includes(|x| x.tokens().where_(|token| token.is_expired.eq(false)))
        .execute(&client)
        .await?;
    assert_eq!(count.total, 4);
    assert_eq!(count.tokens, Some(2));

    let deleted = dinoco::delete::<User>()
        .where_(|x| x.email.eq("returning@dinoco.rs"))
        .returning::<UserSelect>()
        .execute(&client)
        .await?;
    assert_eq!(deleted.len(), 1);

    dinoco::delete::<User>().where_(|x| x.email.eq("many-b@dinoco.rs")).execute(&client).await?;
    assert!(find_first::<User>().where_(|x| x.email.eq("many-b@dinoco.rs")).execute(&client).await?.is_none());

    let deleted_tokens =
        dinoco::delete_many::<Token>().where_(|x| x.user_id.not_null()).returning::<Token>().execute(&client).await?;
    assert_eq!(deleted_tokens.len(), 2);
    dinoco::delete_many::<User>().where_(|x| x.office.batch(vec!["manager", "owner"])).execute(&client).await?;
    assert_eq!(dinoco::count::<User>().execute(&client).await?.total, 0);

    Ok(())
}

async fn run_transactions(client: DinocoClient) -> anyhow::Result<()> {
    let account = TransactionAccount::new("committed".to_string(), "commit@dinoco.rs".to_string());
    let mut committed = transactions(transaction![
        insert_into::<TransactionAccount>().values(&account),
        find_first::<TransactionAccount>()
            .where_(|item| item.id.eq("ignored-before"))
            .where_complex(|item, m| m.and([item.id.eq("committed"), m.not(item.email.eq("blocked@dinoco.rs"))]))
            .where_(|item| item.id.eq("ignored-after")),
        count::<TransactionAccount>(),
    ])
    .execute(&client)
    .await?;
    committed.take::<()>(0)?;
    assert_eq!(
        committed.take::<Option<TransactionAccount>>(1)?.expect("transaction account").email,
        "commit@dinoco.rs"
    );
    assert_eq!(committed.take::<TransactionAccountCount>(2)?.total, 1);

    let first = TransactionAccount::new("duplicate".to_string(), "first@dinoco.rs".to_string());
    let duplicate = TransactionAccount::new("duplicate".to_string(), "second@dinoco.rs".to_string());
    assert!(
        transactions(transaction![
            insert_into::<TransactionAccount>().values(&first),
            insert_into::<TransactionAccount>().values(&duplicate),
        ])
        .execute(&client)
        .await
        .is_err()
    );
    assert!(
        find_first::<TransactionAccount>().where_(|item| item.id.eq("duplicate")).execute(&client).await?.is_none()
    );

    let business = AdapterTransactionBusiness::new("business-1".to_string(), "Before".to_string());
    let system = AdapterTransactionSystem::new("system-1".to_string(), "ERP".to_string());
    insert_into::<AdapterTransactionBusiness>().values(&business).execute(&client).await?;
    insert_into::<AdapterTransactionSystem>().values(&system).execute(&client).await?;

    let mut connected = transactions(transaction![
        update::<AdapterTransactionBusiness>()
            .where_(|item| item.id.eq(&business.id))
            .update(|item| item.system_id.connect(&system.id)),
    ])
    .execute(&client)
    .await?;
    connected.take::<()>(0)?;
    let loaded = find_many::<AdapterTransactionBusiness>()
        .where_(|item| item.id.eq(&business.id))
        .includes(|item| item.systems())
        .execute(&client)
        .await?;
    assert_eq!(loaded[0].systems.len(), 1);

    let duplicate = transactions(transaction![
        update::<AdapterTransactionBusiness>()
            .where_(|item| item.id.eq(&business.id))
            .update(|item| item.name.set("After".to_string()))
            .update(|item| item.system_id.connect(&system.id)),
    ])
    .execute(&client)
    .await;
    assert!(duplicate.is_err());

    let loaded = find_many::<AdapterTransactionBusiness>()
        .where_(|item| item.id.eq(&business.id))
        .includes(|item| item.systems())
        .execute(&client)
        .await?;
    assert_eq!(loaded[0].name, "Before");
    assert_eq!(loaded[0].systems.len(), 1);

    let mut disconnected = transactions(transaction![
        update::<AdapterTransactionBusiness>()
            .where_(|item| item.id.eq(&business.id))
            .update(|item| item.system_id.disconnect(&system.id)),
    ])
    .execute(&client)
    .await?;
    disconnected.take::<()>(0)?;
    let loaded = find_many::<AdapterTransactionBusiness>()
        .where_(|item| item.id.eq(&business.id))
        .includes(|item| item.systems())
        .execute(&client)
        .await?;
    assert!(loaded[0].systems.is_empty());

    let mut finance = AdapterTransactionSystem::new("system-finance".to_string(), "Finance".to_string());
    finance.business_id = Some(business.id.clone());
    let mut extra_systems = vec![
        AdapterTransactionSystem::new("system-bi".to_string(), "BI".to_string()),
        AdapterTransactionSystem::new("system-support".to_string(), "Support".to_string()),
    ];
    for system in &mut extra_systems {
        system.business_id = Some(business.id.clone());
    }
    let mut inserted = transactions(transaction![
        insert_into::<AdapterTransactionSystem>().values(&finance),
        insert_many::<AdapterTransactionSystem>().values(&extra_systems),
    ])
    .execute(&client)
    .await?;
    inserted.take::<()>(0)?;
    inserted.take::<()>(1)?;
    let loaded = find_many::<AdapterTransactionBusiness>()
        .where_(|item| item.id.eq(&business.id))
        .includes(|item| item.systems())
        .execute(&client)
        .await?;
    assert_eq!(loaded[0].systems.len(), 3);

    Ok(())
}

fn monotonic() -> u128 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
}
