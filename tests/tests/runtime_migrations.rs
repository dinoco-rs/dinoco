use dinoco::runtime::{Migration, run_migrations};
use dinoco::{Backend, DinocoAdapter, DinocoClient, SqliteAdapter, rusqlite::Connection};

#[tokio::test]
async fn sqlite_connect_does_not_run_migrations_until_the_programmer_requests_it() -> anyhow::Result<()> {
    let directory = tempfile::tempdir()?;
    let database_path = directory.path().join("local.sqlite");
    let adapter = SqliteAdapter::new(database_path.to_string_lossy().into_owned()).await.map_err(anyhow::Error::msg)?;
    let client = DinocoClient::new(Backend::Sqlite(adapter));
    assert!(!client.backend.logger_enabled(), "SQL logging must be opt-in");
    let client = client.with_logger(true);

    assert!(client.backend.logger_enabled());

    assert!(database_path.exists(), "connect must create the SQLite file");
    let connection = Connection::open(&database_path)?;
    assert_eq!(table_count(&connection, "note")?, 0, "connect must not apply application migrations");
    drop(connection);

    let migrations = [
        Migration::new("001_create_note", "CREATE TABLE note (id INTEGER PRIMARY KEY, body TEXT NOT NULL);"),
        Migration::new("002_seed_note", "INSERT INTO note (body) VALUES ('created at runtime');"),
    ];
    let first = run_migrations(&client, &migrations).await?;
    assert_eq!(first.applied, ["001_create_note", "002_seed_note"]);
    assert!(first.already_applied.is_empty());

    let connection = Connection::open(&database_path)?;
    assert_eq!(table_count(&connection, "note")?, 1);
    assert_eq!(connection.query_row("SELECT COUNT(*) FROM note", [], |row| row.get::<_, i64>(0))?, 1);
    drop(connection);

    let second = run_migrations(&client, &migrations).await?;
    assert!(second.applied.is_empty());
    assert_eq!(second.already_applied, ["001_create_note", "002_seed_note"]);

    Ok(())
}

fn table_count(connection: &Connection, table: &str) -> anyhow::Result<i64> {
    Ok(connection.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
        [table],
        |row| row.get(0),
    )?)
}
