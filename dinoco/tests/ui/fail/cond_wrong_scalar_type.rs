#![allow(dead_code)]

use dinoco::{Model, Projection, Rowable, ScalarField, find_many};

#[derive(Debug, Clone, Rowable)]
struct User {
    id: i64,
    email: String,
}

struct UserWhere {
    id: ScalarField<i64>,
    email: ScalarField<String>,
}

struct UserInclude {}

fn main() {
    let _query = find_many::<User>().cond(|x| x.id.eq("not-an-id"));
}

impl Projection<User> for User {
    fn columns() -> &'static [&'static str] {
        &["id", "email"]
    }
}

impl Model for User {
    type Include = UserInclude;
    type Where = UserWhere;

    fn table_name() -> &'static str {
        "users"
    }
}

impl Default for UserWhere {
    fn default() -> Self {
        Self { id: ScalarField::new("id"), email: ScalarField::new("email") }
    }
}

impl Default for UserInclude {
    fn default() -> Self {
        Self {}
    }
}
