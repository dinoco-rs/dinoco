use std::time::{SystemTime, UNIX_EPOCH};

use dinoco_cli::db::{CliDatabase, DatabaseSchema, DatabaseTable};
use dinoco_cli::sql::{
    MigrationStep, desired_database_schema, generate_create_table_migrations, plan_database_migration,
    plan_schema_migration, plan_sqlite_database_migration,
};
use dinoco_engine::{
    CreateIndexMigration, DinocoAdapter, DinocoSqlCompiler, MigrationColumn, MigrationColumnType, SqliteAdapter,
};

#[tokio::test]
async fn sqlite_introspection_preserves_typed_defaults_without_false_diffs() -> anyhow::Result<()> {
    let path = temp_database("typed-defaults");
    let adapter = SqliteAdapter::new(path.to_string_lossy().to_string()).await.map_err(anyhow::Error::msg)?;
    let schema = dinoco_compiler::compile(DEFAULTS_SCHEMA)?;

    for migration in generate_create_table_migrations(&schema) {
        adapter.execute(&adapter.compile_create_table_migration(migration), &[]).await?;
    }

    let current = CliDatabase::Sqlite(adapter).inspect_schema().await?;
    let unchanged = plan_schema_migration(&schema, &current);
    assert!(unchanged.steps.is_empty(), "an unchanged schema must not produce migration steps: {unchanged:#?}");

    let changed_schema = dinoco_compiler::compile(CHANGED_DEFAULTS_SCHEMA)?;
    let changed = plan_schema_migration(&changed_schema, &current);
    let altered = changed
        .steps
        .iter()
        .filter_map(|step| match step {
            MigrationStep::AlterColumn(migration) => Some(migration.desired.name.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(
        altered,
        ["enabled", "false_text", "number", "numeric_text", "ratio", "text"],
        "every changed default should be detected exactly once"
    );

    drop(current);
    let _ = std::fs::remove_file(path);
    Ok(())
}

#[tokio::test]
async fn sqlite_introspection_preserves_explicit_and_relation_indexes_without_false_diffs() -> anyhow::Result<()> {
    let path = temp_database("indexes");
    let adapter = SqliteAdapter::new(path.to_string_lossy().to_string()).await.map_err(anyhow::Error::msg)?;
    let schema = dinoco_compiler::compile(
        r#"
        model User {
            id    Integer @id @default(autoincrement())
            posts Post[]
        }

        model Post {
            id      Integer @id @default(autoincrement())
            title   String  @index
            user_id Integer
            user    User? @relation(fields: [user_id], references: [id])
        }
        "#,
    )?;
    let desired = desired_database_schema(&schema);

    for migration in generate_create_table_migrations(&schema) {
        adapter.execute(&adapter.compile_create_table_migration(migration), &[]).await?;
    }
    for table in &desired.tables {
        for index in &table.indexes {
            let primary_key_columns = table
                .columns
                .iter()
                .filter(|column| column.primary_key)
                .map(|column| column.name.as_str())
                .collect::<Vec<_>>();
            if index.automatic && index.columns.iter().map(String::as_str).eq(primary_key_columns) {
                continue;
            }
            adapter
                .execute(
                    &adapter.compile_create_index_migration(CreateIndexMigration {
                        table: table.name.clone(),
                        index: index.clone(),
                    }),
                    &[],
                )
                .await?;
        }
    }

    let current = CliDatabase::Sqlite(adapter).inspect_schema().await?;
    let post = current.tables.iter().find(|table| table.name == "post").expect("post table");
    assert!(post.indexes.iter().any(|index| index.name == "idx_post_title"));
    assert!(
        post.indexes
            .iter()
            .any(|index| { index.name == "idx_post_user_id" && index.columns == ["user_id".to_string()] })
    );
    let unchanged = plan_schema_migration(&schema, &current);
    assert!(unchanged.steps.is_empty(), "introspected indexes should match the desired schema: {unchanged:#?}");

    drop(current);
    let _ = std::fs::remove_file(path);
    Ok(())
}

#[test]
fn dropping_an_empty_table_is_still_destructive() {
    let current = DatabaseSchema {
        tables: vec![DatabaseTable {
            name: "address".to_string(),
            row_count: 0,
            columns: vec![MigrationColumn {
                name: "id".to_string(),
                ty: MigrationColumnType::Integer,
                primary_key: true,
                unique: false,
                nullable: false,
                default: None,
            }],
            foreign_keys: Vec::new(),
            indexes: Vec::new(),
        }],
        enums: Vec::new(),
    };

    let plan = plan_database_migration(&DatabaseSchema::default(), &current);

    assert!(plan.steps.iter().any(|step| matches!(step, MigrationStep::DropTable(table) if table.table == "address")));
    assert!(
        plan.warnings.iter().any(|warning| warning.destructive && warning.message.contains("address")),
        "dropping schema is irreversible even when the table currently has no rows"
    );
}

#[test]
fn sqlite_planner_rejects_ambiguous_column_rename_mappings() {
    let make_column = |name: &str| MigrationColumn {
        name: name.to_string(),
        ty: MigrationColumnType::String,
        primary_key: false,
        unique: false,
        nullable: true,
        default: None,
    };
    let current = DatabaseSchema {
        tables: vec![DatabaseTable {
            name: "item".to_string(),
            row_count: 1,
            columns: vec![make_column("first_old_name"), make_column("second_old_name")],
            foreign_keys: Vec::new(),
            indexes: Vec::new(),
        }],
        enums: Vec::new(),
    };
    let desired = DatabaseSchema {
        tables: vec![DatabaseTable {
            name: "item".to_string(),
            row_count: 0,
            columns: vec![make_column("new_name")],
            foreign_keys: Vec::new(),
            indexes: Vec::new(),
        }],
        enums: Vec::new(),
    };

    let plan = plan_sqlite_database_migration(&desired, &current);
    assert_eq!(plan.errors.len(), 1, "an ambiguous rename must be a blocking planner diagnostic");
    assert!(plan.errors[0].contains("first_old_name"));
    assert!(plan.errors[0].contains("second_old_name"));
}

#[tokio::test]
async fn sqlite_migration_statements_roll_back_as_one_unit() -> anyhow::Result<()> {
    let path = temp_database("transaction");
    let adapter = SqliteAdapter::new(path.to_string_lossy().to_string()).await.map_err(anyhow::Error::msg)?;
    let database = CliDatabase::Sqlite(adapter);

    let error = database
        .execute_transaction(&[
            "CREATE TABLE should_rollback (id INTEGER PRIMARY KEY);".to_string(),
            "THIS IS NOT VALID SQL;".to_string(),
        ])
        .await
        .expect_err("the invalid statement must abort the transaction");
    assert!(!error.to_string().is_empty());

    let inspected = database.inspect_schema().await?;
    assert!(
        inspected.tables.iter().all(|table| table.name != "should_rollback"),
        "a failed migration must leave no partially-created table"
    );

    drop(database);
    let _ = std::fs::remove_file(path);
    Ok(())
}

#[tokio::test]
async fn failed_sqlite_rebuild_rolls_back_cascades_and_restores_foreign_keys() -> anyhow::Result<()> {
    let path = temp_database("rebuild-rollback");
    let adapter = SqliteAdapter::new(path.to_string_lossy().to_string()).await.map_err(anyhow::Error::msg)?;
    let database = CliDatabase::Sqlite(adapter);
    database
        .execute_transaction(&[
            "CREATE TABLE parent (id INTEGER PRIMARY KEY);".to_string(),
            "CREATE TABLE child (id INTEGER PRIMARY KEY, parent_id INTEGER NOT NULL REFERENCES parent(id) ON DELETE CASCADE);"
                .to_string(),
            "INSERT INTO parent (id) VALUES (1);".to_string(),
            "INSERT INTO child (id, parent_id) VALUES (1, 1);".to_string(),
        ])
        .await?;

    database
        .execute_transaction(&[
            "/* dinoco-sqlite-table-rebuild: parent */".to_string(),
            "CREATE TABLE __dinoco_rebuild_parent (id INTEGER PRIMARY KEY);".to_string(),
            "INSERT INTO __dinoco_rebuild_parent SELECT * FROM parent;".to_string(),
            "DROP TABLE parent;".to_string(),
            "ALTER TABLE __dinoco_rebuild_parent RENAME TO parent;".to_string(),
            "THIS IS NOT VALID SQL;".to_string(),
        ])
        .await
        .expect_err("the rebuild must roll back atomically");

    let schema = database.inspect_schema().await?;
    assert_eq!(schema.tables.iter().find(|table| table.name == "parent").unwrap().row_count, 1);
    assert_eq!(schema.tables.iter().find(|table| table.name == "child").unwrap().row_count, 1);
    let error = database
        .execute("INSERT INTO child (id, parent_id) VALUES (2, 999)")
        .await
        .expect_err("foreign-key enforcement must be restored after rollback");
    assert!(error.to_string().to_ascii_lowercase().contains("foreign key"), "{error:#}");

    drop(database);
    let _ = std::fs::remove_file(path);
    Ok(())
}

#[tokio::test]
async fn sqlite_managed_migration_rejects_embedded_transaction_control() -> anyhow::Result<()> {
    let path = temp_database("embedded-commit");
    let adapter = SqliteAdapter::new(path.to_string_lossy().to_string()).await.map_err(anyhow::Error::msg)?;
    let database = CliDatabase::Sqlite(adapter);

    let error = database
        .execute_transaction(&[
            "CREATE TABLE must_not_escape (id INTEGER PRIMARY KEY); COMMIT; CREATE TABLE also_forbidden (id INTEGER);"
                .to_string(),
        ])
        .await
        .expect_err("an embedded COMMIT must not escape the managed transaction");
    assert!(!error.to_string().is_empty());

    let inspected = database.inspect_schema().await?;
    assert!(inspected.tables.iter().all(|table| table.name != "must_not_escape"));
    assert!(inspected.tables.iter().all(|table| table.name != "also_forbidden"));

    drop(database);
    let _ = std::fs::remove_file(path);
    Ok(())
}

#[tokio::test]
async fn every_sqlite_pool_connection_enforces_foreign_keys() -> anyhow::Result<()> {
    let path = temp_database("foreign-keys");
    let adapter = SqliteAdapter::new(path.to_string_lossy().to_string()).await.map_err(anyhow::Error::msg)?;
    adapter.execute("CREATE TABLE parent (id TEXT PRIMARY KEY)", &[]).await?;
    adapter
        .execute("CREATE TABLE child (id TEXT PRIMARY KEY, parent_id TEXT NOT NULL REFERENCES parent(id))", &[])
        .await?;

    let error = adapter
        .execute("INSERT INTO child (id, parent_id) VALUES ('child-1', 'missing-parent')", &[])
        .await
        .expect_err("foreign key enforcement must be enabled on pooled connections");
    assert!(error.to_string().to_ascii_lowercase().contains("foreign key"), "{error:#}");

    drop(adapter);
    let _ = std::fs::remove_file(path);
    Ok(())
}

const DEFAULTS_SCHEMA: &str = r#"
config {
    database = "sqlite"
    database_url = env("DATABASE_URL")
}

model Defaults {
    id           Integer  @id @default(autoincrement())
    text         String   @default("hello")
    numeric_text String   @default("1")
    false_text   String   @default("false")
    number       Integer  @default(2)
    ratio        Float    @default(1.5)
    enabled      Boolean  @default(true)
    created_at   DateTime @default(now())
}
"#;

const CHANGED_DEFAULTS_SCHEMA: &str = r#"
config {
    database = "sqlite"
    database_url = env("DATABASE_URL")
}

model Defaults {
    id           Integer  @id @default(autoincrement())
    text         String   @default("world")
    numeric_text String   @default("2")
    false_text   String   @default("true")
    number       Integer  @default(3)
    ratio        Float    @default(2.5)
    enabled      Boolean  @default(false)
    created_at   DateTime @default(now())
}
"#;

fn temp_database(name: &str) -> std::path::PathBuf {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    std::env::temp_dir().join(format!("dinoco-migration-{name}-{}-{nanos}.sqlite", std::process::id()))
}
