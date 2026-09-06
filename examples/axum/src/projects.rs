//! Project endpoints. Between them they exercise `transaction`, `find_many`,
//! `find_first` + `includes`, `count`, `update` and `delete`.

use axum::{Json, extract::Path, extract::State, http::StatusCode};
use dinoco::{
    count, delete as delete_query, find_first, find_many, insert_into, transaction, update as update_query,
};

use crate::{
    database::{Project, Task},
    dto::{CreateProject, ProjectDetail, ProjectResponse, UpdateProject},
    error::ApiError,
    state::AppState,
};

/// `GET /projects` — `find_many` with ordering and an eager `tasks` include.
pub async fn list(State(client): State<AppState>) -> Result<Json<Vec<ProjectResponse>>, ApiError> {
    let projects = find_many::<Project>()
        .order_by(|project| project.name.asc())
        .includes(|project| project.tasks().order_by(|task| task.title.asc()))
        .execute(&client)
        .await
        .map_err(ApiError::internal)?;

    Ok(Json(projects.into_iter().map(ProjectResponse::from).collect()))
}

/// `GET /projects/{id}` — `find_first` for the row plus a `count` for the
/// still-open tasks.
pub async fn get(State(client): State<AppState>, Path(id): Path<String>) -> Result<Json<ProjectDetail>, ApiError> {
    let project = find_first::<Project>()
        .where_(|project| project.id.eq(&id))
        .includes(|project| project.tasks().order_by(|task| task.title.asc()))
        .execute(&client)
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("Project not found"))?;

    let open_tasks = count::<Task>()
        .where_(|task| task.project_id.eq(&id))
        .where_(|task| task.done.eq(false))
        .execute(&client)
        .await
        .map_err(ApiError::internal)?;

    Ok(Json(ProjectDetail { project: project.into(), open_tasks: open_tasks.total }))
}

/// `POST /projects` — create the project and its initial tasks atomically.
/// Any failure rolls the whole thing back.
pub async fn create(
    State(client): State<AppState>,
    Json(payload): Json<CreateProject>,
) -> Result<(StatusCode, Json<ProjectResponse>), ApiError> {
    let name = payload.name.trim().to_string();
    if name.is_empty() {
        return Err(ApiError::bad_request("Project name cannot be empty"));
    }
    if payload.tasks.iter().any(|title| title.trim().is_empty()) {
        return Err(ApiError::bad_request("Task titles cannot be empty"));
    }

    let task_titles = payload.tasks;
    let (project, tasks) = transaction(&client, |tx| async move {
        let project = insert_into::<Project>()
            .value(&Project::new(name))
            .returning::<Project>()
            .execute(tx)
            .await?;

        let mut tasks = Vec::with_capacity(task_titles.len());
        for title in task_titles {
            let mut task = Task::new(title.trim().to_string());
            task.project_id = Some(project.id.clone());
            tasks.push(insert_into::<Task>().value(&task).returning::<Task>().execute(tx).await?);
        }

        Ok((project, tasks))
    })
    .await
    .map_err(ApiError::transaction)?;

    let mut response = ProjectResponse::from(project);
    response.tasks = tasks.into_iter().map(Into::into).collect();

    Ok((StatusCode::CREATED, Json(response)))
}

/// `PATCH /projects/{id}` — `update` with `returning` to echo the new row.
pub async fn update(
    State(client): State<AppState>,
    Path(id): Path<String>,
    Json(payload): Json<UpdateProject>,
) -> Result<Json<ProjectResponse>, ApiError> {
    if payload.is_empty() {
        return Err(ApiError::bad_request("Provide at least one field to update"));
    }

    let mut query = update_query::<Project>().where_(|project| project.id.eq(id));
    if let Some(name) = payload.name {
        let name = name.trim().to_string();
        if name.is_empty() {
            return Err(ApiError::bad_request("Project name cannot be empty"));
        }
        query = query.update(|project| project.name.set(name));
    }
    if let Some(archived) = payload.archived {
        query = query.update(|project| project.archived.set(archived));
    }

    let project = query
        .returning::<Project>()
        .execute(&client)
        .await
        .map_err(ApiError::internal)?
        .into_iter()
        .next()
        .ok_or_else(|| ApiError::not_found("Project not found"))?;

    Ok(Json(project.into()))
}

/// `DELETE /projects/{id}` — remove the tasks and the project in one
/// transaction so a partial delete can never be observed.
pub async fn delete(State(client): State<AppState>, Path(id): Path<String>) -> Result<StatusCode, ApiError> {
    let exists = find_first::<Project>()
        .where_(|project| project.id.eq(&id))
        .execute(&client)
        .await
        .map_err(ApiError::internal)?
        .is_some();
    if !exists {
        return Err(ApiError::not_found("Project not found"));
    }

    transaction(&client, |tx| async move {
        delete_query::<Task>().where_(|task| task.project_id.eq(&id)).execute(tx).await?;
        delete_query::<Project>().where_(|project| project.id.eq(&id)).execute(tx).await?;
        Ok(())
    })
    .await
    .map_err(ApiError::transaction)?;

    Ok(StatusCode::NO_CONTENT)
}
