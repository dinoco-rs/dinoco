//! HTTP glue: read the request body, dispatch on `(method, path)` to a handler
//! in [`crate::handlers`], and serialize the outcome to a JSON response.

use std::convert::Infallible;
use std::sync::Arc;

use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::body::{Body, Incoming};
use hyper::{Method, Request, Response, StatusCode};
use serde::de::DeserializeOwned;
use serde_json::Value;

use dinoco::DinocoClient;

use crate::error::AppError;
use crate::handlers;

const MAX_BODY_BYTES: u64 = 64 * 1024;

pub async fn handle(client: Arc<DinocoClient>, request: Request<Incoming>) -> Result<Response<Full<Bytes>>, Infallible> {
    let (parts, body) = request.into_parts();
    let path = parts.uri.path().to_string();
    let query = parts.uri.query().unwrap_or_default().to_string();
    let segments: Vec<&str> = path.split('/').filter(|segment| !segment.is_empty()).collect();

    let outcome = route(&client, &parts.method, &segments, &query, body).await;

    Ok(match outcome {
        Ok((status, Value::Null)) => empty(status),
        Ok((status, value)) => json(status, &value),
        Err(error) => json(error.status, &handlers::error_body(&error.message)),
    })
}

async fn route(
    client: &DinocoClient,
    method: &Method,
    segments: &[&str],
    query: &str,
    body: Incoming,
) -> Result<(StatusCode, Value), AppError> {
    match (method, segments) {
        (&Method::GET, ["projects"]) => handlers::list_projects(client).await,
        (&Method::POST, ["projects"]) => handlers::create_project(client, read_json(body).await?).await,
        (&Method::GET, ["projects", id]) => handlers::get_project(client, id).await,
        (&Method::PATCH, ["projects", id]) => {
            handlers::update_project(client, (*id).to_string(), read_json(body).await?).await
        }
        (&Method::DELETE, ["projects", id]) => handlers::delete_project(client, (*id).to_string()).await,
        (&Method::POST, ["projects", id, "tasks"]) => {
            handlers::create_task(client, (*id).to_string(), read_json(body).await?).await
        }
        (&Method::GET, ["tasks"]) => handlers::list_tasks(client, handlers::parse_task_filter(query)).await,
        (&Method::GET, ["tasks", id]) => handlers::get_task(client, id).await,
        (&Method::PATCH, ["tasks", id]) => {
            handlers::update_task(client, (*id).to_string(), read_json(body).await?).await
        }
        (&Method::DELETE, ["tasks", id]) => handlers::delete_task(client, (*id).to_string()).await,
        _ => Err(AppError::not_found("Route not found")),
    }
}

async fn read_json<T: DeserializeOwned>(body: Incoming) -> Result<T, AppError> {
    let upper = body.size_hint().upper().unwrap_or(u64::MAX);
    if upper > MAX_BODY_BYTES {
        return Err(AppError::bad_request("Request body too large"));
    }

    let bytes = body.collect().await.map_err(|error| AppError::bad_request(error.to_string()))?.to_bytes();
    if bytes.len() as u64 > MAX_BODY_BYTES {
        return Err(AppError::bad_request("Request body too large"));
    }

    serde_json::from_slice(&bytes).map_err(|error| AppError::bad_request(format!("Invalid JSON: {error}")))
}

fn json(status: StatusCode, value: &Value) -> Response<Full<Bytes>> {
    let body = serde_json::to_vec(value).unwrap_or_else(|_| b"{\"error\":\"serialization failed\"}".to_vec());
    Response::builder()
        .status(status)
        .header(hyper::header::CONTENT_TYPE, "application/json")
        .body(Full::new(Bytes::from(body)))
        .expect("valid response")
}

fn empty(status: StatusCode) -> Response<Full<Bytes>> {
    Response::builder().status(status).body(Full::new(Bytes::new())).expect("valid response")
}
