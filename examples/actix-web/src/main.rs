mod dto;
mod error;
mod projects;
mod routes;
mod state;
mod tasks;

#[path = "../dinoco/mod.rs"]
mod database;

use std::sync::Arc;

use actix_web::{App, HttpServer, web};
use state::AppState;

#[actix_web::main]
async fn main() -> anyhow::Result<()> {
    let state: AppState = Arc::new(database::connect().await?);
    println!("Actix Web example listening on http://127.0.0.1:3001");

    HttpServer::new(move || App::new().app_data(web::Data::new(state.clone())).configure(routes::configure))
        .bind(("127.0.0.1", 3001))?
        .run()
        .await?;

    Ok(())
}
