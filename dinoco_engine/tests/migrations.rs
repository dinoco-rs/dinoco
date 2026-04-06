mod common;

use dinoco_engine::{
    DinocoAdapterHandler, DinocoClient, MigrationExecutor, MySqlAdapter, PostgresAdapter, SqliteAdapter,
};

use crate::common::{
    alter_enum_schema, alter_enum_step, apply_sqls, migration_schema, migration_steps, mysql_url, postgres_url,
    sqlite_url, unique_name,
};

#[tokio::test]
async fn sqlite_migration_builds_and_applies_schema() {
    let prefix = unique_name("mig");
    let schema = migration_schema(&prefix);
    let steps = migration_steps(&prefix);
    let client = DinocoClient::<SqliteAdapter>::new(sqlite_url("migrations"), vec![])
        .await
        .expect("sqlite client should connect");

    client.primary().reset_database().await.expect("sqlite database should reset");
    let sqls = client.primary().build_migration(&steps, &schema, false);

    apply_sqls(client.primary(), &sqls).await.expect("sqlite migration should apply");

    let tables = client.primary().fetch_tables().await.expect("tables should load");
    let users_table = format!("{prefix}_users");
    let users = tables.iter().find(|table| table.name == users_table).expect("users table should exist");

    assert!(users.columns.iter().any(|column| column.name == "status"));
    assert!(users.columns.iter().any(|column| column.name == "email"));

    let foreign_keys = client.primary().fetch_foreign_keys().await.expect("foreign keys should load");

    assert!(foreign_keys.iter().any(|fk| fk.table_name == users_table && fk.column_name == "team_id"));

    let indexes = client.primary().fetch_indexes().await.expect("indexes should load");

    assert!(indexes.iter().any(|index| index.table_name == users_table && index.column_name == "status"));
    assert!(
        indexes.iter().any(|index| index.table_name == users_table && index.column_name == "email" && index.is_unique)
    );
}

#[tokio::test]
async fn postgres_migration_builds_and_applies_schema() {
    let prefix = unique_name("mig");
    let schema = migration_schema(&prefix);
    let steps = migration_steps(&prefix);
    let client =
        DinocoClient::<PostgresAdapter>::new(postgres_url(), vec![]).await.expect("postgres client should connect");

    client.primary().reset_database().await.expect("postgres database should reset");
    let sqls = client.primary().build_migration(&steps, &schema, false);

    apply_sqls(client.primary(), &sqls).await.expect("postgres migration should apply");

    let tables = client.primary().fetch_tables().await.expect("tables should load");
    let users_table = format!("{prefix}_users");
    let users = tables.iter().find(|table| table.name == users_table).expect("users table should exist");

    assert!(users.columns.iter().any(|column| column.name == "status" && column.db_type == format!("{prefix}_status")));
    assert!(users.columns.iter().any(|column| column.name == "email"));

    let enums = client.primary().fetch_enums().await.expect("enums should load");

    assert!(enums.iter().any(|item| item.name == format!("{prefix}_status") && item.value == "ACTIVE"));
    assert!(enums.iter().any(|item| item.name == format!("{prefix}_status") && item.value == "DISABLED"));

    let foreign_keys = client.primary().fetch_foreign_keys().await.expect("foreign keys should load");

    assert!(foreign_keys.iter().any(|fk| fk.table_name == users_table && fk.column_name == "team_id"));

    let indexes = client.primary().fetch_indexes().await.expect("indexes should load");

    assert!(indexes.iter().any(|index| index.table_name == users_table && index.column_name == "status"));
    assert!(
        indexes.iter().any(|index| index.table_name == users_table && index.column_name == "email" && index.is_unique)
    );
}

#[tokio::test]
async fn mysql_migration_builds_and_applies_schema() {
    let prefix = unique_name("mig");
    let schema = migration_schema(&prefix);
    let steps = migration_steps(&prefix);
    let client = DinocoClient::<MySqlAdapter>::new(mysql_url(), vec![]).await.expect("mysql client should connect");

    client.primary().reset_database().await.expect("mysql database should reset");
    let sqls = client.primary().build_migration(&steps, &schema, false);

    apply_sqls(client.primary(), &sqls).await.expect("mysql migration should apply");

    let tables = client.primary().fetch_tables().await.expect("tables should load");
    let users_table = format!("{prefix}_users");
    let users = tables.iter().find(|table| table.name == users_table).expect("users table should exist");

    assert!(users.columns.iter().any(|column| column.name == "status" && column.db_type.starts_with("enum(")));
    assert!(users.columns.iter().any(|column| column.name == "email"));

    let foreign_keys = client.primary().fetch_foreign_keys().await.expect("foreign keys should load");

    assert!(foreign_keys.iter().any(|fk| fk.table_name == users_table && fk.column_name == "team_id"));

    let indexes = client.primary().fetch_indexes().await.expect("indexes should load");

    assert!(indexes.iter().any(|index| index.table_name == users_table && index.column_name == "status"));
    assert!(
        indexes.iter().any(|index| index.table_name == users_table && index.column_name == "email" && index.is_unique)
    );
}

#[test]
fn postgres_alter_enum_migration_rebuilds_type_when_variants_are_removed() {
    let prefix = unique_name("enum");
    let schema = alter_enum_schema(&prefix);
    let adapter = PostgresAdapter {
        url: String::new(),
        client: std::sync::Arc::new(
            deadpool_postgres::Pool::builder(deadpool_postgres::Manager::new(
                tokio_postgres::Config::new(),
                tokio_postgres::NoTls,
            ))
            .max_size(1)
            .build()
            .expect("pool should build"),
        ),
    };

    let sqls = adapter.build_migration(&[alter_enum_step(&prefix)], &schema, false);

    assert!(sqls.iter().any(|sql| sql.contains("ALTER TYPE") && sql.contains("RENAME TO")));
    assert!(sqls.iter().any(|sql| sql.contains("CREATE TYPE")));
    assert!(sqls.iter().any(|sql| sql.contains("ALTER TABLE") && sql.contains("ALTER COLUMN")));
    assert!(sqls.iter().any(|sql| sql.contains("DROP TYPE")));
}

#[test]
fn mysql_alter_enum_migration_cleans_up_rows_before_modifying_column() {
    let prefix = unique_name("enum");
    let schema = alter_enum_schema(&prefix);
    let url = mysql_url();
    let adapter = MySqlAdapter { url: url.clone(), client: std::sync::Arc::new(mysql_async::Pool::new(url.as_str())) };

    let sqls = adapter.build_migration(&[alter_enum_step(&prefix)], &schema, false);

    assert!(sqls.iter().any(|sql| sql.starts_with("UPDATE ")));
    assert!(sqls.iter().any(|sql| sql.contains("MODIFY COLUMN")));
}

#[test]
fn sqlite_alter_enum_migration_rebuilds_the_table() {
    let prefix = unique_name("enum");
    let schema = alter_enum_schema(&prefix);
    let adapter = SqliteAdapter {
        url: String::new(),
        pool: std::sync::Arc::new(
            deadpool_sqlite::Config::new(":memory:")
                .create_pool(deadpool_sqlite::Runtime::Tokio1)
                .expect("pool should build"),
        ),
    };

    let sqls = adapter.build_migration(&[alter_enum_step(&prefix)], &schema, false);

    assert!(sqls.iter().any(|sql| sql == "PRAGMA foreign_keys = OFF;"));
    assert!(sqls.iter().any(|sql| sql.contains("__dinoco_rebuild_")));
    assert!(sqls.iter().any(|sql| sql == "PRAGMA foreign_keys = ON;"));
}
