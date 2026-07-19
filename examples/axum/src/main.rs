mod dto;
mod error;
mod routes;
mod state;
mod todos;

#[path = "../dinoco/mod.rs"]
mod database;

use std::sync::Arc;

use state::AppState;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let state: AppState = Arc::new(database::connect().await?);
    let app = routes::router(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000").await?;
    println!("Axum example listening on http://127.0.0.1:3000");
    axum::serve(listener, app).await?;

    Ok(())
}
