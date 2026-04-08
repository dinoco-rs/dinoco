#![allow(dead_code)]
#![allow(non_snake_case)]

use dinoco::{DateTimeUtc, Extend, Model, Projection, Rowable, ScalarField, Utc};

#[derive(Debug, Clone, Rowable)]
struct Post {
    id: String,
    name: String,
    likes: i64,
    createdAt: DateTimeUtc<Utc>,
}

struct PostWhere {
    id: ScalarField<String>,
    name: ScalarField<String>,
    likes: ScalarField<i64>,
    createdAt: ScalarField<DateTimeUtc<Utc>>,
}

struct PostInclude {}

#[derive(Debug, Clone, Extend)]
#[extend(Post)]
struct PostListItem {
    id: String,
    name: String,
    likes: i64,
    createdAt: String,
}

fn main() {}

impl Projection<Post> for Post {
    fn columns() -> &'static [&'static str] {
        &["id", "name", "likes", "createdAt"]
    }
}

impl Model for Post {
    type Include = PostInclude;
    type Where = PostWhere;

    fn table_name() -> &'static str {
        "posts"
    }
}

impl Default for PostWhere {
    fn default() -> Self {
        Self {
            id: ScalarField::new("id"),
            name: ScalarField::new("name"),
            likes: ScalarField::new("likes"),
            createdAt: ScalarField::new("createdAt"),
        }
    }
}

impl Default for PostInclude {
    fn default() -> Self {
        Self {}
    }
}
