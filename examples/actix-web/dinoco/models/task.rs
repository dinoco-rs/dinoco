#[allow(unused_imports)]
use super::*;
use dinoco::Entity;

#[derive(Debug, Clone, Entity, ::dinoco::serde::Serialize, ::dinoco::serde::Deserialize)]
#[serde(crate = "::dinoco::serde")]
#[dinoco(table_name = "task")]
pub struct Task {
    #[dinoco(primary_key, auto_generate = uuid)]
    pub id: ::dinoco::Uuid,

    pub project_id: Option<::dinoco::Uuid>,

    pub title: String,

    #[dinoco(default = false)]
    pub done: bool,

    #[dinoco(many_to_one, foreign_key = "project_id", references = "id")]
    pub project: Option<Project>,

}
