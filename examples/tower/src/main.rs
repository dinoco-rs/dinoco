//! A raw `tower` + `hyper` example. The request pipeline is a `tower` stack —
//! a hand-written `LogLayer`, plus `ConcurrencyLimit` and `Timeout` from
//! `tower` itself — wrapping a `tower::service_fn` that runs Dinoco queries.

mod app;
mod dto;
mod error;
mod handlers;
mod log_layer;

#[path = "../dinoco/mod.rs"]
mod database;

use std::sync::Arc;
use std::time::Duration;

use hyper::server::conn::http1;
use hyper_util::rt::TokioIo;
use hyper_util::service::TowerToHyperService;
use log_layer::LogLayer;
use tokio::net::TcpListener;
use tower::ServiceBuilder;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = Arc::new(database::connect().await?);

    // One `tower` service for every connection. `service_fn` is the leaf; the
    // `ServiceBuilder` layers wrap it outermost-first.
    let service = {
        let client = client.clone();
        ServiceBuilder::new()
            .layer(LogLayer)
            .concurrency_limit(128)
            .timeout(Duration::from_secs(15))
            .service_fn(move |request| {
                let client = client.clone();
                async move { app::handle(client, request).await }
            })
    };
    let service = TowerToHyperService::new(service);

    let listener = TcpListener::bind(("127.0.0.1", 3002)).await?;
    println!("Tower example listening on http://127.0.0.1:3002");

    loop {
        let (stream, _) = listener.accept().await?;
        let io = TokioIo::new(stream);
        let service = service.clone();

        tokio::spawn(async move {
            if let Err(error) = http1::Builder::new().serve_connection(io, service).await {
                eprintln!("connection error: {error}");
            }
        });
    }
}
