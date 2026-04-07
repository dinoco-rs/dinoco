use serde_json::Value;
use tuono_lib::{Request, Response};

use tuono_lib::axum::http::{HeaderMap, StatusCode, header};

fn get_latest_version_path() -> String {
    let versions: Value =
        serde_json::from_str(include_str!("../jsons/versions.json")).expect("Failed to parse versions.json");

    let version_name = versions
        .as_array()
        .and_then(|entries| entries.first())
        .and_then(|entry| entry.get("name"))
        .and_then(Value::as_str)
        .unwrap();

    format!("/{version_name}")
}

#[tuono_lib::handler]
async fn handler(_req: Request) -> Response {
    let mut headers = HeaderMap::new();
    let path = get_latest_version_path();

    headers.insert(header::LOCATION, path.parse().unwrap());

    Response::Custom((StatusCode::TEMPORARY_REDIRECT, headers, String::new()))
}
