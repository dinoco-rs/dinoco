//! Every Dinoco operation the example demonstrates lives here, decoupled from
//! HTTP: each function takes the client plus already-parsed inputs and returns
//! `(StatusCode, json)` or an [`AppError`].

use dinoco::{
    DinocoClient, count, delete as delete_query, find_and_update, find_first, find_many, insert_into, transaction,
    update as update_query,
};
use hyper::StatusCode;
use serde_json::{Value, json};

use crate::database::{Project, Task};
use crate::dto::{CreateProject, CreateTask, ProjectDetail, ProjectResponse, TaskFilter, TaskResponse, UpdateProject, UpdateTask};
use crate::error::AppError;

fn ok_json<T: serde::Serialize>(status: StatusCode, value: T) -> Result<(StatusCode, Value), AppError> {
    let value = serde_json::to_value(value).map_err(|error| AppError::internal(error.into()))?;
    Ok((status, value))
}

// ------------------------------- projects ----------------------------------

/// `find_many` with ordering and an eager `tasks` include.
pub async fn list_projects(client: &DinocoClient) -> Result<(StatusCode, Value), AppError> {
    let projects = find_many::<Project>()
        .order_by(|project| project.name.asc())
        .includes(|project| project.tasks().order_by(|task| task.title.asc()))
        .execute(client)
        .await
        .map_err(AppError::internal)?;

    ok_json(StatusCode::OK, projects.into_iter().map(ProjectResponse::from).collect::<Vec<_>>())
}

/// `find_first` for the row plus a `count` of the still-open tasks.
pub async fn get_project(client: &DinocoClient, id: &str) -> Result<(StatusCode, Value), AppError> {
    let project = find_first::<Project>()
        .where_(|project| project.id.eq(id))
        .includes(|project| project.tasks().order_by(|task| task.title.asc()))
        .execute(client)
        .await
        .map_err(AppError::internal)?
        .ok_or_else(|| AppError::not_found("Project not found"))?;

    let open_tasks = count::<Task>()
        .where_(|task| task.project_id.eq(id))
        .where_(|task| task.done.eq(false))
        .execute(client)
        .await
        .map_err(AppError::internal)?;

    ok_json(StatusCode::OK, ProjectDetail { project: project.into(), open_tasks: open_tasks.total })
}

/// Create the project and its initial tasks atomically.
pub async fn create_project(client: &DinocoClient, payload: CreateProject) -> Result<(StatusCode, Value), AppError> {
    let name = payload.name.trim().to_string();
    if name.is_empty() {
        return Err(AppError::bad_request("Project name cannot be empty"));
    }
    if payload.tasks.iter().any(|title| title.trim().is_empty()) {
        return Err(AppError::bad_request("Task titles cannot be empty"));
    }

    let task_titles = payload.tasks;
    let (project, tasks) = transaction(client, |tx| async move {
        let project =
            insert_into::<Project>().value(&Project::new(name)).returning::<Project>().execute(tx).await?;

        let mut tasks = Vec::with_capacity(task_titles.len());
        for title in task_titles {
            let mut task = Task::new(title.trim().to_string());
            task.project_id = Some(project.id.clone());
            tasks.push(insert_into::<Task>().value(&task).returning::<Task>().execute(tx).await?);
        }

        Ok((project, tasks))
    })
    .await
    .map_err(AppError::transaction)?;

    let mut response = ProjectResponse::from(project);
    response.tasks = tasks.into_iter().map(TaskResponse::from).collect();

    ok_json(StatusCode::CREATED, response)
}

/// `update` with `returning` to echo the new row.
pub async fn update_project(
    client: &DinocoClient,
    id: String,
    payload: UpdateProject,
) -> Result<(StatusCode, Value), AppError> {
    if payload.is_empty() {
        return Err(AppError::bad_request("Provide at least one field to update"));
    }

    let mut query = update_query::<Project>().where_(|project| project.id.eq(id));
    if let Some(name) = payload.name {
        let name = name.trim().to_string();
        if name.is_empty() {
            return Err(AppError::bad_request("Project name cannot be empty"));
        }
        query = query.update(|project| project.name.set(name));
    }
    if let Some(archived) = payload.archived {
        query = query.update(|project| project.archived.set(archived));
    }

    let project = query
        .returning::<Project>()
        .execute(client)
        .await
        .map_err(AppError::internal)?
        .into_iter()
        .next()
        .ok_or_else(|| AppError::not_found("Project not found"))?;

    ok_json(StatusCode::OK, ProjectResponse::from(project))
}

/// Delete the tasks and the project in one transaction.
pub async fn delete_project(client: &DinocoClient, id: String) -> Result<(StatusCode, Value), AppError> {
    let exists = find_first::<Project>()
        .where_(|project| project.id.eq(&id))
        .execute(client)
        .await
        .map_err(AppError::internal)?
        .is_some();
    if !exists {
        return Err(AppError::not_found("Project not found"));
    }

    transaction(client, |tx| async move {
        delete_query::<Task>().where_(|task| task.project_id.eq(&id)).execute(tx).await?;
        delete_query::<Project>().where_(|project| project.id.eq(&id)).execute(tx).await?;
        Ok(())
    })
    .await
    .map_err(AppError::transaction)?;

    Ok((StatusCode::NO_CONTENT, Value::Null))
}

// -------------------------------- tasks ------------------------------------

/// `find_many` where each query-string field contributes one optional `where_`.
pub async fn list_tasks(client: &DinocoClient, filter: TaskFilter) -> Result<(StatusCode, Value), AppError> {
    let mut query = find_many::<Task>();
    if let Some(project_id) = filter.project_id {
        query = query.where_(move |task| task.project_id.eq(project_id));
    }
    if let Some(done) = filter.done {
        query = query.where_(move |task| task.done.eq(done));
    }

    let tasks = query
        .order_by(|task| task.title.asc())
        .take(100)
        .execute(client)
        .await
        .map_err(AppError::internal)?;

    ok_json(StatusCode::OK, tasks.into_iter().map(TaskResponse::from).collect::<Vec<_>>())
}

/// A single `find_first` lookup.
pub async fn get_task(client: &DinocoClient, id: &str) -> Result<(StatusCode, Value), AppError> {
    let task = find_first::<Task>()
        .where_(|task| task.id.eq(id))
        .execute(client)
        .await
        .map_err(AppError::internal)?
        .ok_or_else(|| AppError::not_found("Task not found"))?;

    ok_json(StatusCode::OK, TaskResponse::from(task))
}

/// `insert_into` a single row, after checking the parent project exists.
pub async fn create_task(
    client: &DinocoClient,
    project_id: String,
    payload: CreateTask,
) -> Result<(StatusCode, Value), AppError> {
    let title = payload.title.trim().to_string();
    if title.is_empty() {
        return Err(AppError::bad_request("Task title cannot be empty"));
    }

    let project_exists = find_first::<Project>()
        .where_(|project| project.id.eq(&project_id))
        .execute(client)
        .await
        .map_err(AppError::internal)?
        .is_some();
    if !project_exists {
        return Err(AppError::not_found("Project not found"));
    }

    let mut task = Task::new(title);
    task.project_id = Some(project_id);
    let task = insert_into::<Task>()
        .value(&task)
        .returning::<Task>()
        .execute(client)
        .await
        .map_err(AppError::internal)?;

    ok_json(StatusCode::CREATED, TaskResponse::from(task))
}

/// `find_and_update` returns the updated row or a typed `RowNotAffected`.
pub async fn update_task(
    client: &DinocoClient,
    id: String,
    payload: UpdateTask,
) -> Result<(StatusCode, Value), AppError> {
    if payload.is_empty() {
        return Err(AppError::bad_request("Provide at least one field to update"));
    }

    let mut query = find_and_update::<Task>().where_(|task| task.id.eq(id));
    if let Some(title) = payload.title {
        let title = title.trim().to_string();
        if title.is_empty() {
            return Err(AppError::bad_request("Task title cannot be empty"));
        }
        query = query.update(|task| task.title.set(title));
    }
    if let Some(done) = payload.done {
        query = query.update(|task| task.done.set(done));
    }

    let task = query.execute(client).await.map_err(AppError::atomic)?;

    ok_json(StatusCode::OK, TaskResponse::from(task))
}

/// `delete` with `returning` so a missing row is a 404.
pub async fn delete_task(client: &DinocoClient, id: String) -> Result<(StatusCode, Value), AppError> {
    let removed = delete_query::<Task>()
        .where_(|task| task.id.eq(id))
        .returning::<Task>()
        .execute(client)
        .await
        .map_err(AppError::internal)?;

    if removed.is_empty() {
        return Err(AppError::not_found("Task not found"));
    }

    Ok((StatusCode::NO_CONTENT, Value::Null))
}

/// Turns `?project_id=..&done=true` into a [`TaskFilter`].
pub fn parse_task_filter(query: &str) -> TaskFilter {
    let mut filter = TaskFilter { project_id: None, done: None };

    for pair in query.split('&').filter(|pair| !pair.is_empty()) {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        match key {
            "project_id" => filter.project_id = Some(value.to_string()),
            "done" => filter.done = Some(matches!(value, "1" | "true")),
            _ => {}
        }
    }

    filter
}

/// Small helper so `handlers` can build a JSON body without pulling in the HTTP
/// layer. Also used for the top-level error body.
pub fn error_body(message: &str) -> Value {
    json!({ "error": message })
}
