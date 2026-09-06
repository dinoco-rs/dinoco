#![allow(unused)]

pub mod models;

pub use models::*;

pub async fn connect() -> ::dinoco::anyhow::Result<::dinoco::DinocoClient> {
    let database_url = std::env::var("DATABASE_URL")?;
    let adapter = <::dinoco::SqliteAdapter as ::dinoco::DinocoAdapter>::new(database_url).await.map_err(::dinoco::anyhow::Error::msg)?;
    let client = ::dinoco::DinocoClient::new(::dinoco::Backend::Sqlite(adapter));
    let read_replicas = vec![
    ];
    Ok(client.with_read_replicas(read_replicas).with_logger(false))
}
