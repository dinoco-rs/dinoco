use std::env;

use dinoco_derives::Rowable;
use dinoco_engine::{
    DinocoAdapter, DinocoClientConfig, DinocoError, DinocoResult, MySqlAdapter, PostgresAdapter, SqliteAdapter,
};
use uuid::Uuid;

#[derive(Rowable, Debug)]
struct DinocoMigration {
    pub name: String,
    pub applied_at: Option<String>,
    pub rollback_at: Option<String>,
}

#[allow(dead_code)]
#[path = "../../../dinoco_cli/src/helpers/database.rs"]
mod database;

#[tokio::test]
async fn migration_history_roundtrip_works_for_mysql() {
    let migration_name = format!("20260406_{}", Uuid::now_v7().simple());
    let adapter = match MySqlAdapter::connect(mysql_url(), DinocoClientConfig::default()).await {
        Ok(adapter) => adapter,
        Err(err) if should_skip_external_adapter_test(&err) => {
            eprintln!("skipping mysql migration history roundtrip test: {err}");
            return;
        }
        Err(err) => panic!("mysql adapter should connect: {err}"),
    };

    if let Err(err) = assert_migration_history_roundtrip(&adapter, &migration_name).await {
        if should_skip_external_adapter_test(&err) {
            eprintln!("skipping mysql migration history roundtrip test: {err}");
            return;
        }

        panic!("mysql migration history should roundtrip: {err}");
    }
}

#[tokio::test]
async fn migration_history_roundtrip_works_for_postgres() {
    let migration_name = format!("20260406_{}", Uuid::now_v7().simple());
    let adapter = match PostgresAdapter::connect(postgres_url(), DinocoClientConfig::default()).await {
        Ok(adapter) => adapter,
        Err(err) if should_skip_external_adapter_test(&err) => {
            eprintln!("skipping postgres migration history roundtrip test: {err}");
            return;
        }
        Err(err) => panic!("postgres adapter should connect: {err}"),
    };

    if let Err(err) = assert_migration_history_roundtrip(&adapter, &migration_name).await {
        if should_skip_external_adapter_test(&err) {
            eprintln!("skipping postgres migration history roundtrip test: {err}");
            return;
        }

        panic!("postgres migration history should roundtrip: {err}");
    }
}

#[tokio::test]
async fn migration_history_roundtrip_works_for_sqlite() {
    let migration_name = format!("20260406_{}", Uuid::now_v7().simple());
    let adapter = SqliteAdapter::connect(sqlite_url("migration-history-roundtrip"), DinocoClientConfig::default())
        .await
        .expect("sqlite adapter should connect");

    assert_migration_history_roundtrip(&adapter, &migration_name)
        .await
        .expect("sqlite migration history should roundtrip");
}

async fn assert_migration_history_roundtrip<T: DinocoAdapter>(adapter: &T, migration_name: &str) -> DinocoResult<()> {
    database::create_migration_table(adapter).await?;
    database::insert_migration(adapter, migration_name).await?;

    let pending =
        database::get_migration_by_name(adapter, migration_name).await?.expect("migration should exist after insert");

    assert_eq!(pending.name, migration_name);
    assert!(pending.applied_at.is_none());
    assert!(pending.rollback_at.is_none());

    database::mark_migration_applied(adapter, migration_name).await?;

    let applied =
        database::get_migration_by_name(adapter, migration_name).await?.expect("migration should exist after update");

    assert_eq!(applied.name, migration_name);
    assert!(applied.applied_at.is_some());
    assert!(applied.rollback_at.is_none());

    Ok(())
}

fn mysql_url() -> String {
    env::var("DINOCO_MYSQL_DATABASE_URL")
        .or_else(|_| env::var("MYSQL_DATABASE_URL"))
        .or_else(|_| env::var("DATABASE_URL"))
        .unwrap_or_else(|_| "mysql://root:root@localhost:3306/dinoco".to_string())
}

fn postgres_url() -> String {
    env::var("DINOCO_POSTGRES_DATABASE_URL")
        .or_else(|_| env::var("POSTGRES_DATABASE_URL"))
        .or_else(|_| env::var("DATABASE_URL"))
        .unwrap_or_else(|_| "postgres://postgres:root@localhost:5432/dinoco".to_string())
}

fn sqlite_url(name: &str) -> String {
    let mut path = env::temp_dir();

    path.push(format!("dinoco-cli-{name}-{}.sqlite", Uuid::now_v7()));

    format!("file:{}", path.display())
}

fn should_skip_external_adapter_test(error: &DinocoError) -> bool {
    match error {
        DinocoError::ConnectionError(_) => true,
        DinocoError::MySql(mysql_error) => mysql_error.to_string().contains("Operation not permitted"),
        DinocoError::Postgres(postgres_error) => postgres_error.to_string().contains("error connecting to server"),
        _ => false,
    }
}
