use actix_web::{HttpResponse, web};
use dinoco::{delete as delete_query, find_first, find_many, insert_into, update as update_query};

use crate::{
    database::Todo,
    dto::{CreateTodo, TodoResponse, UpdateTodo},
    error::ApiError,
    state::AppState,
};

pub async fn list_all(client: web::Data<AppState>) -> Result<web::Json<Vec<TodoResponse>>, ApiError> {
    let todos = find_many::<Todo>().execute(client.get_ref()).await.map_err(ApiError::internal)?;
    Ok(web::Json(todos.into_iter().map(Into::into).collect()))
}

pub async fn list_by_id(
    client: web::Data<AppState>,
    id: web::Path<String>,
) -> Result<web::Json<TodoResponse>, ApiError> {
    let todo = find_first::<Todo>()
        .where_(|todo| todo.id.eq(id.into_inner()))
        .execute(client.get_ref())
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(ApiError::not_found)?;

    Ok(web::Json(todo.into()))
}

pub async fn create(client: web::Data<AppState>, payload: web::Json<CreateTodo>) -> Result<HttpResponse, ApiError> {
    if payload.title.trim().is_empty() {
        return Err(ApiError::bad_request("Title cannot be empty"));
    }

    let todo = Todo::new(payload.title.clone());
    let todo = insert_into::<Todo>()
        .values(&todo)
        .returning::<Todo>()
        .execute(client.get_ref())
        .await
        .map_err(ApiError::internal)?;

    Ok(HttpResponse::Created().json(TodoResponse::from(todo)))
}

pub async fn update(
    client: web::Data<AppState>,
    id: web::Path<String>,
    payload: web::Json<UpdateTodo>,
) -> Result<web::Json<TodoResponse>, ApiError> {
    if payload.is_empty() {
        return Err(ApiError::bad_request("Provide at least one field to update"));
    }
    if payload.title.as_ref().is_some_and(|title| title.trim().is_empty()) {
        return Err(ApiError::bad_request("Title cannot be empty"));
    }

    let mut query = update_query::<Todo>().where_(|todo| todo.id.eq(id.into_inner()));
    if let Some(title) = payload.title.clone() {
        query = query.update(|todo| todo.title.set(title));
    }
    if let Some(completed) = payload.completed {
        query = query.update(|todo| todo.completed.set(completed));
    }

    let todo = query
        .returning::<Todo>()
        .execute(client.get_ref())
        .await
        .map_err(ApiError::internal)?
        .into_iter()
        .next()
        .ok_or_else(ApiError::not_found)?;

    Ok(web::Json(todo.into()))
}

pub async fn delete(client: web::Data<AppState>, id: web::Path<String>) -> Result<HttpResponse, ApiError> {
    let deleted = delete_query::<Todo>()
        .where_(|todo| todo.id.eq(id.into_inner()))
        .returning::<Todo>()
        .execute(client.get_ref())
        .await
        .map_err(ApiError::internal)?;

    if deleted.is_empty() {
        return Err(ApiError::not_found());
    }

    Ok(HttpResponse::NoContent().finish())
}
