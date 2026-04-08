#![allow(non_snake_case)]

use std::collections::HashMap;
use std::env;

use dinoco::{
    DinocoAdapter, DinocoClient, DinocoGenericRow, DinocoResult, DinocoRow, Extend, IncludeLoaderFuture,
    IntoDinocoValue, Model, Projection, RelationField, Rowable, ScalarField, find_many,
};
use uuid::Uuid;

#[derive(Debug, Clone, Rowable)]
struct User {
    id: i64,
    name: String,
}

struct UserWhere {
    id: ScalarField<i64>,
    name: ScalarField<String>,
}

struct UserInclude {}

#[derive(Debug, Clone, Rowable)]
struct Post {
    id: String,
    name: String,
    likes: i64,
    authorId: i64,
}

struct PostWhere {
    id: ScalarField<String>,
    name: ScalarField<String>,
    likes: ScalarField<i64>,
    authorId: ScalarField<i64>,
}

struct PostInclude {}

#[derive(Debug, Clone, Extend)]
#[extend(Post)]
struct PostListItem {
    id: String,
    name: String,
    likes: i64,
}

#[derive(Debug, Clone, Extend)]
#[extend(User)]
struct UserWithPosts {
    id: i64,
    name: String,
    posts: Vec<PostListItem>,
}

fn sqlite_url(name: &str) -> String {
    let mut path = env::temp_dir();

    path.push(format!("dinoco-include-limit-{name}-{}.sqlite", Uuid::now_v7()));

    format!("file:{}", path.display())
}

#[tokio::test]
async fn include_take_is_applied_per_parent() -> DinocoResult<()> {
    let client = DinocoClient::<dinoco_engine::SqliteAdapter>::new(
        sqlite_url("per-parent"),
        vec![],
        dinoco::DinocoClientConfig::default(),
    )
    .await?;

    client
        .primary()
        .execute(
            r#"CREATE TABLE "users" (
                "id" INTEGER PRIMARY KEY,
                "name" TEXT NOT NULL,
                "email" TEXT NOT NULL UNIQUE,
                "age" INTEGER,
                "role" TEXT NOT NULL,
                "active" BOOLEAN NOT NULL,
                "createdAt" DATETIME NOT NULL
            )"#,
            &[],
        )
        .await?;

    client
        .primary()
        .execute(
            r#"CREATE TABLE "Post" (
                "id" TEXT PRIMARY KEY,
                "name" TEXT NOT NULL,
                "content" TEXT,
                "likes" INTEGER NOT NULL,
                "status" TEXT NOT NULL,
                "createdAt" DATETIME NOT NULL,
                "authorId" INTEGER NOT NULL
            )"#,
            &[],
        )
        .await?;

    client
        .primary()
        .execute(
            r#"INSERT INTO "users" ("id", "name", "email", "age", "role", "active", "createdAt")
               VALUES
               (1, 'User 1', 'user1@dinoco.dev', NULL, 'MEMBER', 1, CURRENT_TIMESTAMP),
               (2, 'User 2', 'user2@dinoco.dev', NULL, 'MEMBER', 1, CURRENT_TIMESTAMP)"#,
            &[],
        )
        .await?;

    client
        .primary()
        .execute(
            r#"INSERT INTO "Post" ("id", "name", "content", "likes", "status", "createdAt", "authorId")
               VALUES
               ('u1-p1', 'A1', NULL, 1, 'PUBLISHED', CURRENT_TIMESTAMP, 1),
               ('u1-p2', 'A2', NULL, 2, 'PUBLISHED', CURRENT_TIMESTAMP, 1),
               ('u1-p3', 'A3', NULL, 3, 'PUBLISHED', CURRENT_TIMESTAMP, 1),
               ('u2-p1', 'B1', NULL, 1, 'PUBLISHED', CURRENT_TIMESTAMP, 2),
               ('u2-p2', 'B2', NULL, 2, 'PUBLISHED', CURRENT_TIMESTAMP, 2),
               ('u2-p3', 'B3', NULL, 3, 'PUBLISHED', CURRENT_TIMESTAMP, 2)"#,
            &[],
        )
        .await?;

    let users = find_many::<User>()
        .select::<UserWithPosts>()
        .order_by(|x| x.id.asc())
        .take(10)
        .includes(|x| x.posts().take(2).order_by(|post| post.name.asc()).select::<PostListItem>())
        .execute(&client)
        .await?;

    assert_eq!(users.len(), 2);
    assert_eq!(users[0].posts.iter().map(|item| item.name.as_str()).collect::<Vec<_>>(), vec!["A1", "A2"]);
    assert_eq!(users[1].posts.iter().map(|item| item.name.as_str()).collect::<Vec<_>>(), vec!["B1", "B2"]);

    Ok(())
}

impl User {
    pub fn __dinoco_load_posts<'a, P, C, A>(
        item_keys: Vec<Option<i64>>,
        include: &'a dinoco::IncludeNode,
        client: &'a DinocoClient<A>,
        read_mode: dinoco::ReadMode,
        relation_field: impl Fn(&mut P) -> &mut Vec<C> + Copy + 'a,
    ) -> IncludeLoaderFuture<'a, P>
    where
        A: DinocoAdapter,
        C: Projection<Post> + Clone,
    {
        Box::pin(async move {
            struct PartitionedChildRow<C> {
                item: C,
                relation_key: i64,
            }

            impl<C> DinocoRow for PartitionedChildRow<C>
            where
                C: Projection<Post>,
            {
                fn from_row<R: DinocoGenericRow>(row: &R) -> DinocoResult<Self> {
                    Ok(Self { item: C::from_row(row)?, relation_key: row.get(C::columns().len())? })
                }
            }

            let keys = item_keys.iter().flatten().cloned().collect::<Vec<_>>();

            if keys.is_empty() {
                return Ok(Box::new(|_: &mut [P]| {}) as dinoco::IncludeApplier<'a, P>);
            }

            let adapter = client.read_adapter(false);
            let base_statement = include
                .statement
                .clone()
                .unwrap_or_else(|| dinoco_engine::SelectStatement::new().from("Post").select(C::columns()));
            let mut statement = base_statement;
            let mut select_columns = statement.select.clone();

            if select_columns.is_empty() {
                select_columns = C::columns().iter().map(|column| format!("{}.{}", "Post", column)).collect::<Vec<_>>();
            } else {
                select_columns = select_columns
                    .into_iter()
                    .map(|column| {
                        if column.contains('.')
                            || column.contains(' ')
                            || column.contains('(')
                            || column.contains(')')
                            || column.contains(',')
                        {
                            column
                        } else {
                            format!("{}.{}", "Post", column)
                        }
                    })
                    .collect::<Vec<_>>();
            }

            select_columns.push(format!("{}.{}", "Post", "authorId"));
            statement.select = select_columns;
            statement.conditions.push(
                dinoco_engine::Expression::Column(format!("{}.{}", "Post", "authorId"))
                    .in_values(keys.iter().cloned().map(IntoDinocoValue::into_dinoco_value).collect()),
            );

            let (sql, params) = dinoco_engine::QueryBuilder::build_partitioned_select(
                adapter.dialect(),
                &statement,
                "authorId",
                "__dinoco_row_num",
            );
            let child_rows = adapter.query_as::<PartitionedChildRow<C>>(&sql, &params).await?;
            let relation_keys = child_rows.iter().map(|row| row.relation_key).collect::<Vec<_>>();
            let mut children = child_rows.into_iter().map(|row| row.item).collect::<Vec<_>>();

            C::load_includes(&mut children, &include.includes, client, read_mode).await?;

            let mut grouped: HashMap<i64, Vec<C>> = HashMap::new();

            for (relation_key, child) in relation_keys.into_iter().zip(children.into_iter()) {
                grouped.entry(relation_key).or_default().push(child);
            }

            Ok(Box::new(move |items: &mut [P]| {
                for (item, key) in items.iter_mut().zip(item_keys.into_iter()) {
                    *relation_field(item) = key.and_then(|key| grouped.remove(&key)).unwrap_or_default();
                }
            }) as dinoco::IncludeApplier<'a, P>)
        })
    }
}

impl Projection<User> for User {
    fn columns() -> &'static [&'static str] {
        &["id", "name"]
    }
}

impl Projection<Post> for Post {
    fn columns() -> &'static [&'static str] {
        &["id", "name", "likes", "authorId"]
    }
}

impl Model for User {
    type Include = UserInclude;
    type Where = UserWhere;

    fn table_name() -> &'static str {
        "users"
    }
}

impl Model for Post {
    type Include = PostInclude;
    type Where = PostWhere;

    fn table_name() -> &'static str {
        "Post"
    }
}

impl Default for UserWhere {
    fn default() -> Self {
        Self { id: ScalarField::new("id"), name: ScalarField::new("name") }
    }
}

impl Default for PostWhere {
    fn default() -> Self {
        Self {
            id: ScalarField::new("id"),
            name: ScalarField::new("name"),
            likes: ScalarField::new("likes"),
            authorId: ScalarField::new("authorId"),
        }
    }
}

impl Default for UserInclude {
    fn default() -> Self {
        Self {}
    }
}

impl Default for PostInclude {
    fn default() -> Self {
        Self {}
    }
}

impl UserInclude {
    fn posts(&self) -> RelationField<Post> {
        RelationField::new("posts")
    }
}
