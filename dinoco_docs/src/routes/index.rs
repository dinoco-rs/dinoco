use std::fs;
use std::path::Path;

use tuono_lib::{Request, Response};

use tuono_lib::axum::http::{HeaderMap, StatusCode, header};

fn get_latest_version_path() -> String {
    let versions_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/jsons/versions");

    let mut versions = fs::read_dir(versions_path)
        .expect("Failed to read versions directory")
        .filter_map(Result::ok)
        .filter(|entry| entry.path().is_dir())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .collect::<Vec<_>>();

    versions.sort();

    format!(
        "/{}",
        versions.last().expect("No versions found in src/jsons/versions")
    )
}

#[tuono_lib::handler]
async fn handler(_req: Request) -> Response {
    let mut headers = HeaderMap::new();
    let path = get_latest_version_path();

    headers.insert(header::LOCATION, path.parse().unwrap());

    Response::Custom((StatusCode::TEMPORARY_REDIRECT, headers, String::new()))
}
