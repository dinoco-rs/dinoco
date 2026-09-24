//! Route tests: each one runs the real router against `database::connect()`,
//! which under `cfg(test)` is an in-memory SQLite copy of
//! `dinoco/schema.dinoco`, and uses `setup_test_methods::<M>` to assert what
//! the route did to the database.

use std::sync::{Arc, Mutex};

use axum::{
    Router,
    body::Body,
    http::{Method, Request, StatusCode},
};
use dinoco::{find_many, insert_into, remove_test_methods, setup_test_methods};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;

use crate::{
    database::{self, Project, Task},
    routes,
    state::AppState,
};

async fn app() -> anyhow::Result<(Router, AppState)> {
    // The same `connect()` `main` uses: under `cfg(test)` it is the test ambient.
    let state: AppState = Arc::new(database::connect().await?);
    Ok((routes::router(state.clone()), state))
}

async fn send(app: &Router, method: Method, uri: &str, body: Option<Value>) -> anyhow::Result<(StatusCode, Value)> {
    let request = Request::builder().method(method).uri(uri).header("content-type", "application/json");
    let request = request.body(body.map_or_else(Body::empty, |body| Body::from(body.to_string())))?;
    let response = app.clone().oneshot(request).await?;
    let status = response.status();
    let bytes = response.into_body().collect().await?.to_bytes();
    let body = if bytes.is_empty() { Value::Null } else { serde_json::from_slice(&bytes)? };

    Ok((status, body))
}

#[tokio::test]
async fn create_project_inserts_the_project_and_its_tasks_in_one_transaction() -> anyhow::Result<()> {
    let (app, client) = app().await?;
    let inserts = Arc::new(Mutex::new(Vec::new()));

    let projects = inserts.clone();
    setup_test_methods::<Project>(&client).on_insert(move |rows, query, error| {
        assert!(error.is_none(), "{error:?}");
        projects.lock().unwrap().push((query.table, rows.map(<[_]>::len), query.in_transaction));
    });
    let tasks = inserts.clone();
    setup_test_methods::<Task>(&client).on_insert(move |rows, query, _| {
        tasks.lock().unwrap().push((query.table, rows.map(<[_]>::len), query.in_transaction));
    });

    let (status, body) =
        send(&app, Method::POST, "/projects", Some(json!({ "name": "Launch", "tasks": ["Write docs", "Ship"] })))
            .await?;

    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["tasks"].as_array().map(Vec::len), Some(2));
    assert_eq!(
        *inserts.lock().unwrap(),
        [("project", Some(1), true), ("task", Some(1), true), ("task", Some(1), true)],
    );

    Ok(())
}

#[tokio::test]
async fn create_project_rejects_an_empty_name_before_touching_the_database() -> anyhow::Result<()> {
    let (app, client) = app().await?;
    let touched = Arc::new(Mutex::new(false));

    let flag = touched.clone();
    setup_test_methods::<Project>(&client).on_insert(move |_, _, _| *flag.lock().unwrap() = true);

    let (status, body) = send(&app, Method::POST, "/projects", Some(json!({ "name": "   " }))).await?;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "Project name cannot be empty");
    assert!(!*touched.lock().unwrap(), "no INSERT for an invalid payload");

    Ok(())
}

#[tokio::test]
async fn create_task_for_a_missing_project_is_a_404_without_insert() -> anyhow::Result<()> {
    let (app, client) = app().await?;
    let events = Arc::new(Mutex::new(Vec::new()));

    let finds = events.clone();
    setup_test_methods::<Project>(&client)
        .on_find(move |rows, query, _| finds.lock().unwrap().push(format!("find {} {rows:?}", query.table)));
    let inserts = events.clone();
    setup_test_methods::<Task>(&client)
        .on_insert(move |_, query, _| inserts.lock().unwrap().push(format!("insert {}", query.table)));

    let (status, _) = send(&app, Method::POST, "/projects/missing/tasks", Some(json!({ "title": "Orphan" }))).await?;

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(*events.lock().unwrap(), ["find project Some(0)"]);

    Ok(())
}

#[tokio::test]
async fn list_projects_loads_tasks_through_one_include_query() -> anyhow::Result<()> {
    let (app, client) = app().await?;
    for name in ["Beta", "Alpha"] {
        let project = insert_into::<Project>()
            .value(&Project::new(name.to_string()))
            .returning::<Project>()
            .execute(&*client)
            .await?;
        let mut task = Task::new(format!("{name} task"));
        task.project_id = Some(project.id.clone());
        insert_into::<Task>().value(&task).execute(&*client).await?;
    }

    let task_queries = Arc::new(Mutex::new(0));
    let counter = task_queries.clone();
    setup_test_methods::<Task>(&client).on_find(move |_, _, _| *counter.lock().unwrap() += 1);

    let (status, body) = send(&app, Method::GET, "/projects", None).await?;

    assert_eq!(status, StatusCode::OK);
    let names = body.as_array().unwrap().iter().map(|project| project["name"].clone()).collect::<Vec<_>>();
    assert_eq!(names, [json!("Alpha"), json!("Beta")]);
    assert_eq!(*task_queries.lock().unwrap(), 1, "tasks come from a single include query, not one per project");

    remove_test_methods::<Task>(&client);
    Ok(())
}

#[tokio::test]
async fn delete_task_reports_one_deleted_row_then_404() -> anyhow::Result<()> {
    let (app, client) = app().await?;
    let task =
        insert_into::<Task>().value(&Task::new("Temporary".to_string())).returning::<Task>().execute(&*client).await?;
    let deleted = Arc::new(Mutex::new(Vec::new()));

    let log = deleted.clone();
    setup_test_methods::<Task>(&client).on_delete(move |affected, _, _| log.lock().unwrap().push(affected));

    let (first, _) = send(&app, Method::DELETE, &format!("/tasks/{}", task.id), None).await?;
    let (second, _) = send(&app, Method::DELETE, &format!("/tasks/{}", task.id), None).await?;

    assert_eq!(first, StatusCode::NO_CONTENT);
    assert_eq!(second, StatusCode::NOT_FOUND);
    assert_eq!(*deleted.lock().unwrap(), [Some(1), Some(0)]);

    Ok(())
}

#[tokio::test]
async fn connect_returns_an_isolated_test_database_under_cfg_test() -> anyhow::Result<()> {
    // No DATABASE_URL is needed: `connect()` never reaches the real database here.
    let first = database::connect().await?;
    let second = database::connect().await?;

    insert_into::<Project>().value(&Project::new("Only in first".to_string())).execute(&first).await?;

    assert_eq!(find_many::<Project>().execute(&first).await?.len(), 1);
    assert!(find_many::<Project>().execute(&second).await?.is_empty());

    Ok(())
}
