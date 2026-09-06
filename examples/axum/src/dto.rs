use serde::{Deserialize, Serialize};

use crate::database::{Project, Task};

// ---------------------------------------------------------------------------
// Requests
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct CreateProject {
    pub name: String,
    /// Initial task titles, inserted together with the project in one
    /// transaction.
    #[serde(default)]
    pub tasks: Vec<String>,
}

#[derive(Deserialize)]
pub struct UpdateProject {
    pub name: Option<String>,
    pub archived: Option<bool>,
}

impl UpdateProject {
    pub fn is_empty(&self) -> bool {
        self.name.is_none() && self.archived.is_none()
    }
}

#[derive(Deserialize)]
pub struct CreateTask {
    pub title: String,
}

#[derive(Deserialize)]
pub struct UpdateTask {
    pub title: Option<String>,
    pub done: Option<bool>,
}

impl UpdateTask {
    pub fn is_empty(&self) -> bool {
        self.title.is_none() && self.done.is_none()
    }
}

#[derive(Deserialize)]
pub struct TaskFilter {
    pub project_id: Option<String>,
    pub done: Option<bool>,
}

// ---------------------------------------------------------------------------
// Responses
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct TaskResponse {
    pub id: String,
    pub project_id: Option<String>,
    pub title: String,
    pub done: bool,
}

impl From<Task> for TaskResponse {
    fn from(task: Task) -> Self {
        Self { id: task.id, project_id: task.project_id, title: task.title, done: task.done }
    }
}

#[derive(Serialize)]
pub struct ProjectResponse {
    pub id: String,
    pub name: String,
    pub archived: bool,
    pub tasks: Vec<TaskResponse>,
}

impl From<Project> for ProjectResponse {
    fn from(project: Project) -> Self {
        Self {
            id: project.id,
            name: project.name,
            archived: project.archived,
            tasks: project.tasks.into_iter().map(TaskResponse::from).collect(),
        }
    }
}

#[derive(Serialize)]
pub struct ProjectDetail {
    #[serde(flatten)]
    pub project: ProjectResponse,
    /// Populated with `count::<Task>()` filtered to the still-open tasks.
    pub open_tasks: i64,
}
