use serde::{Deserialize, Serialize};

use crate::database::Todo;

#[derive(Deserialize)]
pub struct CreateTodo {
    pub title: String,
}

#[derive(Deserialize)]
pub struct UpdateTodo {
    pub title: Option<String>,
    pub completed: Option<bool>,
}

impl UpdateTodo {
    pub fn is_empty(&self) -> bool {
        self.title.is_none() && self.completed.is_none()
    }
}

#[derive(Serialize)]
pub struct TodoResponse {
    pub id: String,
    pub title: String,
    pub completed: bool,
}

impl From<Todo> for TodoResponse {
    fn from(todo: Todo) -> Self {
        Self { id: todo.id, title: todo.title, completed: todo.completed }
    }
}
