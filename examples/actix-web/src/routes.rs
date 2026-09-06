use actix_web::web;

use crate::{projects, tasks};

pub fn configure(config: &mut web::ServiceConfig) {
    config
        .route("/projects", web::get().to(projects::list))
        .route("/projects", web::post().to(projects::create))
        .route("/projects/{id}", web::get().to(projects::get))
        .route("/projects/{id}", web::patch().to(projects::update))
        .route("/projects/{id}", web::delete().to(projects::delete))
        .route("/projects/{id}/tasks", web::post().to(tasks::create))
        .route("/tasks", web::get().to(tasks::list))
        .route("/tasks/{id}", web::get().to(tasks::get))
        .route("/tasks/{id}", web::patch().to(tasks::update))
        .route("/tasks/{id}", web::delete().to(tasks::delete));
}
