#![allow(dead_code)]

use dinoco::{Extend, Model, Projection, ScalarField};

#[derive(Default)]
struct UserWhere {
    id: ScalarField<i64>,
}

#[derive(Default)]
struct UserInclude {}

#[derive(Debug, Clone, Extend)]
#[extend(User)]
struct User {
    id: i64,
    #[dinoco_virtual]
    display_name: Option<String>,
    #[dinoco_virtual]
    score: i64,
}

impl Model for User {
    type Include = UserInclude;
    type Where = UserWhere;

    fn table_name() -> &'static str {
        "users"
    }
}

fn main() {
    let _ = dinoco::find_first::<User>();
    assert_eq!(<User as Projection<User>>::columns(), &["id"]);
}
