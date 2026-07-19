#[allow(unused_imports)]
use super::*;
use dinoco::Entity;

#[derive(Debug, Entity)]
#[dinoco(table_name = "todo")]
pub struct Todo {
    #[dinoco(primary_key, auto_generate = uuid)]
    pub id: ::dinoco::Uuid,

    pub title: String,

    #[dinoco(default = false)]
    pub completed: bool,
}
