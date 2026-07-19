use axum::{Router, routing::get};

use crate::{state::AppState, todos};

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/todos", get(todos::list_all).post(todos::create))
        .route("/todos/{id}", get(todos::list_by_id).put(todos::update).delete(todos::delete))
        .with_state(state)
}
