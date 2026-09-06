//! Task endpoints: `find_many` with dynamic filters, `insert_into` (create),
//! `find_and_update` (atomic update) and `delete`.

use actix_web::{HttpResponse, web};
use dinoco::{delete as delete_query, find_and_update, find_first, find_many, insert_into};

use crate::{
    database::{Project, Task},
    dto::{CreateTask, TaskFilter, TaskResponse, UpdateTask},
    error::ApiError,
    state::AppState,
};

/// `GET /tasks?project_id=&done=` — `find_many` where each query-string field
/// contributes one optional `where_` clause.
pub async fn list(
    client: web::Data<AppState>,
    filter: web::Query<TaskFilter>,
) -> Result<web::Json<Vec<TaskResponse>>, ApiError> {
    let filter = filter.into_inner();
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
        .execute(client.get_ref())
        .await
        .map_err(ApiError::internal)?;

    Ok(web::Json(tasks.into_iter().map(TaskResponse::from).collect()))
}

/// `GET /tasks/{id}` — a single `find_first` lookup.
pub async fn get(client: web::Data<AppState>, id: web::Path<String>) -> Result<web::Json<TaskResponse>, ApiError> {
    let task = find_first::<Task>()
        .where_(|task| task.id.eq(id.into_inner()))
        .execute(client.get_ref())
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("Task not found"))?;

    Ok(web::Json(task.into()))
}

/// `POST /projects/{id}/tasks` — `insert_into` a single row, after checking the
/// parent project exists.
pub async fn create(
    client: web::Data<AppState>,
    project_id: web::Path<String>,
    payload: web::Json<CreateTask>,
) -> Result<HttpResponse, ApiError> {
    let title = payload.into_inner().title.trim().to_string();
    if title.is_empty() {
        return Err(ApiError::bad_request("Task title cannot be empty"));
    }

    let project_id = project_id.into_inner();
    let project_exists = find_first::<Project>()
        .where_(|project| project.id.eq(&project_id))
        .execute(client.get_ref())
        .await
        .map_err(ApiError::internal)?
        .is_some();
    if !project_exists {
        return Err(ApiError::not_found("Project not found"));
    }

    let mut task = Task::new(title);
    task.project_id = Some(project_id);
    let task = insert_into::<Task>()
        .value(&task)
        .returning::<Task>()
        .execute(client.get_ref())
        .await
        .map_err(ApiError::internal)?;

    Ok(HttpResponse::Created().json(TaskResponse::from(task)))
}

/// `PATCH /tasks/{id}` — `find_and_update` returns the updated row or a typed
/// `RowNotAffected` that maps to 404.
pub async fn update(
    client: web::Data<AppState>,
    id: web::Path<String>,
    payload: web::Json<UpdateTask>,
) -> Result<web::Json<TaskResponse>, ApiError> {
    let payload = payload.into_inner();
    if payload.is_empty() {
        return Err(ApiError::bad_request("Provide at least one field to update"));
    }

    let mut query = find_and_update::<Task>().where_(|task| task.id.eq(id.into_inner()));
    if let Some(title) = payload.title {
        let title = title.trim().to_string();
        if title.is_empty() {
            return Err(ApiError::bad_request("Task title cannot be empty"));
        }
        query = query.update(|task| task.title.set(title));
    }
    if let Some(done) = payload.done {
        query = query.update(|task| task.done.set(done));
    }

    let task = query.execute(client.get_ref()).await.map_err(ApiError::atomic)?;

    Ok(web::Json(task.into()))
}

/// `DELETE /tasks/{id}` — `delete` with `returning` so a missing row is a 404.
pub async fn delete(client: web::Data<AppState>, id: web::Path<String>) -> Result<HttpResponse, ApiError> {
    let removed = delete_query::<Task>()
        .where_(|task| task.id.eq(id.into_inner()))
        .returning::<Task>()
        .execute(client.get_ref())
        .await
        .map_err(ApiError::internal)?;

    if removed.is_empty() {
        return Err(ApiError::not_found("Task not found"));
    }

    Ok(HttpResponse::NoContent().finish())
}
