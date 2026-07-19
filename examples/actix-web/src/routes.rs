use actix_web::web;

use crate::todos;

pub fn configure(config: &mut web::ServiceConfig) {
    config
        .route("/todos", web::get().to(todos::list_all))
        .route("/todos", web::post().to(todos::create))
        .route("/todos/{id}", web::get().to(todos::list_by_id))
        .route("/todos/{id}", web::put().to(todos::update))
        .route("/todos/{id}", web::delete().to(todos::delete));
}
