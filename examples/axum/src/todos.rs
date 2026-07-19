use axum::{Json, extract::Path, extract::State, http::StatusCode};
use dinoco::{delete as delete_query, find_first, find_many, insert_into, update as update_query};

use crate::{
    database::Todo,
    dto::{CreateTodo, TodoResponse, UpdateTodo},
    error::ApiError,
    state::AppState,
};

pub async fn list_all(State(client): State<AppState>) -> Result<Json<Vec<TodoResponse>>, ApiError> {
    let todos = find_many::<Todo>().execute(&client).await.map_err(ApiError::internal)?;
    Ok(Json(todos.into_iter().map(Into::into).collect()))
}

pub async fn list_by_id(
    State(client): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<TodoResponse>, ApiError> {
    let todo = find_first::<Todo>()
        .where_(|todo| todo.id.eq(id))
        .execute(&client)
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(ApiError::not_found)?;

    Ok(Json(todo.into()))
}

pub async fn create(
    State(client): State<AppState>,
    Json(payload): Json<CreateTodo>,
) -> Result<(StatusCode, Json<TodoResponse>), ApiError> {
    if payload.title.trim().is_empty() {
        return Err(ApiError::bad_request("Title cannot be empty"));
    }

    let todo = Todo::new(payload.title);
    let todo =
        insert_into::<Todo>().values(&todo).returning::<Todo>().execute(&client).await.map_err(ApiError::internal)?;

    Ok((StatusCode::CREATED, Json(todo.into())))
}

pub async fn update(
    State(client): State<AppState>,
    Path(id): Path<String>,
    Json(payload): Json<UpdateTodo>,
) -> Result<Json<TodoResponse>, ApiError> {
    if payload.is_empty() {
        return Err(ApiError::bad_request("Provide at least one field to update"));
    }
    if payload.title.as_ref().is_some_and(|title| title.trim().is_empty()) {
        return Err(ApiError::bad_request("Title cannot be empty"));
    }

    let mut query = update_query::<Todo>().where_(|todo| todo.id.eq(id));
    if let Some(title) = payload.title {
        query = query.update(|todo| todo.title.set(title));
    }
    if let Some(completed) = payload.completed {
        query = query.update(|todo| todo.completed.set(completed));
    }

    let todo = query
        .returning::<Todo>()
        .execute(&client)
        .await
        .map_err(ApiError::internal)?
        .into_iter()
        .next()
        .ok_or_else(ApiError::not_found)?;

    Ok(Json(todo.into()))
}

pub async fn delete(State(client): State<AppState>, Path(id): Path<String>) -> Result<StatusCode, ApiError> {
    let deleted = delete_query::<Todo>()
        .where_(|todo| todo.id.eq(id))
        .returning::<Todo>()
        .execute(&client)
        .await
        .map_err(ApiError::internal)?;

    if deleted.is_empty() {
        return Err(ApiError::not_found());
    }

    Ok(StatusCode::NO_CONTENT)
}
