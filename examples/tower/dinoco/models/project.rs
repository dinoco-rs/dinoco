#[allow(unused_imports)]
use super::*;
use dinoco::Entity;

#[derive(Debug, Clone, Entity, ::dinoco::serde::Serialize, ::dinoco::serde::Deserialize)]
#[serde(crate = "::dinoco::serde")]
#[dinoco(table_name = "project")]
pub struct Project {
    #[dinoco(primary_key, auto_generate = uuid)]
    pub id: ::dinoco::Uuid,

    pub name: String,

    #[dinoco(default = false)]
    pub archived: bool,

    #[dinoco(one_to_many, foreign_key = "project_id", references = "id")]
    pub tasks: Vec<Task>,

}
