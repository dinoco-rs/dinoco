use dinoco::{Model, Projection, Rowable, ScalarField, delete};

#[derive(Debug, Clone, Rowable)]
struct User {
    id: i64,
}

struct UserWhere {
    id: ScalarField<i64>,
}

struct UserInclude {}

fn main() {
    let _ = delete::<User>().execute;
}

impl Projection<User> for User {
    fn columns() -> &'static [&'static str] {
        &["id"]
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
        Self { id: ScalarField::new("id") }
    }
}

impl Default for UserInclude {
    fn default() -> Self {
        Self {}
    }
}
