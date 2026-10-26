use std::future::Future;

use dinoco_engine::{Backend, DinocoClient};

/// Runs the project's seed for `dinoco seed`, from the test that the generated
/// `dinoco/mod.rs` declares.
///
/// `dinoco seed` compiles the application's own crate for its tests, so the
/// seed sees every item of the crate under `crate::`, and runs only that test,
/// with the database it resolved for the workspace in `DINOCO_SEED_*`. Without
/// them, as under a plain `cargo test -- --ignored`, nothing runs.
#[doc(hidden)]
pub fn __run_seed<F, Fut>(seed: F) -> anyhow::Result<()>
where
    F: FnOnce(DinocoClient) -> Fut,
    Fut: Future<Output = anyhow::Result<()>>,
{
    let Some(report) = std::env::var_os("DINOCO_SEED_REPORT") else {
        return Ok(());
    };
    // A crate whose library and binaries all include `dinoco/mod.rs` declares
    // the test once per target; the seed runs only in the first.
    let report = std::path::PathBuf::from(report);
    if report.exists() {
        return Ok(());
    }

    // The runtime `#[tokio::main]` builds, so application code the seed calls
    // (`block_in_place`, tasks it waits on) behaves as it does in the app.
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build()?;
    runtime.block_on(async {
        let client = connect().await?;
        seed(client).await
    })?;
    std::fs::write(report, "")?;
    Ok(())
}

async fn connect() -> anyhow::Result<DinocoClient> {
    let var = |name: &str| {
        std::env::var(name).map_err(|_| anyhow::anyhow!("{name} is not set: run the seed with `dinoco seed`"))
    };
    let url = var("DINOCO_SEED_DATABASE_URL")?;
    let backend = match (var("DINOCO_SEED_DATABASE")?.as_str(), var("DINOCO_SEED_CONNECTION")?.as_str()) {
        #[cfg(feature = "postgres")]
        ("postgresql", "pgbouncer") => Backend::PgBouncer(dinoco_engine::PgBouncerAdapter::new(url).await?),
        #[cfg(feature = "postgres")]
        ("postgresql", _) => Backend::Postgres(dinoco_engine::PostgresAdapter::direct(url).await?),
        #[cfg(feature = "mysql")]
        ("mysql", _) => Backend::Mysql(dinoco_engine::MySqlAdapter::new(url)),
        #[cfg(feature = "sqlite")]
        ("sqlite", _) => Backend::Sqlite(
            <dinoco_engine::SqliteAdapter as dinoco_engine::DinocoAdapter>::new(url)
                .await
                .map_err(anyhow::Error::msg)?,
        ),
        (database, _) => {
            let feature = if database == "postgresql" { "postgres" } else { database };
            anyhow::bail!(
                "the `{feature}` feature of `dinoco` is not enabled in this crate, so the seed cannot connect"
            )
        }
    };
    Ok(DinocoClient::new(backend))
}
