use axum::{
    Router,
    routing::{get, post},
};

use crate::{projects, state::AppState, tasks};

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/projects", get(projects::list).post(projects::create))
        .route("/projects/{id}", get(projects::get).patch(projects::update).delete(projects::delete))
        .route("/projects/{id}/tasks", post(tasks::create))
        .route("/tasks", get(tasks::list))
        .route("/tasks/{id}", get(tasks::get).patch(tasks::update).delete(tasks::delete))
        .with_state(state)
}
