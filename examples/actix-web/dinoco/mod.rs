#![allow(unused)]

pub mod models;

pub use models::*;

/// Connects to the database configured in `schema.dinoco`. When this crate is
/// compiled for its own tests (`cfg(test)`), it returns [`connect_test`]
/// instead, so code under test never reaches the real database.
pub async fn connect() -> ::dinoco::anyhow::Result<::dinoco::DinocoClient> {
    if cfg!(test) {
        return connect_test().await;
    }

    connect_database().await
}

/// A fresh in-memory SQLite database with every table of `schema.dinoco`,
/// isolated from every other call. See `dinoco::create_test_ambient`.
pub async fn connect_test() -> ::dinoco::anyhow::Result<::dinoco::DinocoClient> {
    ::dinoco::TestAmbient::new()
        .schema(concat!(env!("CARGO_MANIFEST_DIR"), "/dinoco/schema.dinoco"))
        .create()
        .await
}

/// Connects to the configured database, even under `cfg(test)`.
pub async fn connect_database() -> ::dinoco::anyhow::Result<::dinoco::DinocoClient> {
    let database_url = std::env::var("DATABASE_URL")?;
    let adapter = <::dinoco::SqliteAdapter as ::dinoco::DinocoAdapter>::new(database_url).await.map_err(::dinoco::anyhow::Error::msg)?;
    let client = ::dinoco::DinocoClient::new(::dinoco::Backend::Sqlite(adapter));
    let read_replicas = vec![
    ];
    Ok(client.with_read_replicas(read_replicas).with_logger(false).with_query_mode(::dinoco::QueryMode::BatchQuery))
}
