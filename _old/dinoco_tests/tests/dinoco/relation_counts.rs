#![allow(non_snake_case)]

use std::collections::HashMap;
use std::env;

use dinoco::{
    DinocoAdapter, DinocoClient, DinocoGenericRow, DinocoResult, DinocoRow, Extend, IncludeLoaderFuture,
    IntoDinocoValue, Model, Projection, RelationField, Rowable, ScalarField, find_first, find_many,
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
    id: i64,
    title: String,
    published: bool,
    authorId: i64,
}

struct PostWhere {
    id: ScalarField<i64>,
    title: ScalarField<String>,
    published: ScalarField<bool>,
    authorId: ScalarField<i64>,
}

struct PostInclude {}

#[derive(Debug, Clone, Rowable)]
struct Comment {
    id: i64,
    text: String,
    flagged: bool,
    postId: i64,
}

struct CommentWhere {
    id: ScalarField<i64>,
    text: ScalarField<String>,
    flagged: ScalarField<bool>,
    postId: ScalarField<i64>,
}

struct CommentInclude {}

#[derive(Debug, Clone, Extend)]
#[extend(Comment)]
struct CommentListItem {
    id: i64,
    text: String,
}

#[derive(Debug, Clone, Extend)]
#[extend(Post)]
struct PostListItem {
    id: i64,
    title: String,
    comments_count: usize,
    comments: Vec<CommentListItem>,
}

#[derive(Debug, Clone, Extend)]
#[extend(User)]
struct UserListItem {
    id: i64,
    name: String,
    posts_count: usize,
    posts: Vec<PostListItem>,
}

fn sqlite_url(name: &str) -> String {
    let mut path = env::temp_dir();

    path.push(format!("dinoco-relation-counts-{name}-{}.sqlite", Uuid::now_v7()));

    format!("file:{}", path.display())
}

async fn create_tables(client: &DinocoClient<dinoco_engine::SqliteAdapter>) -> DinocoResult<()> {
    client
        .primary()
        .execute(
            r#"CREATE TABLE "users" (
                "id" INTEGER PRIMARY KEY,
                "name" TEXT NOT NULL
            )"#,
            &[],
        )
        .await?;

    client
        .primary()
        .execute(
            r#"CREATE TABLE "posts" (
                "id" INTEGER PRIMARY KEY,
                "title" TEXT NOT NULL,
                "published" BOOLEAN NOT NULL,
                "authorId" INTEGER NOT NULL
            )"#,
            &[],
        )
        .await?;

    client
        .primary()
        .execute(
            r#"CREATE TABLE "comments" (
                "id" INTEGER PRIMARY KEY,
                "text" TEXT NOT NULL,
                "flagged" BOOLEAN NOT NULL,
                "postId" INTEGER NOT NULL
            )"#,
            &[],
        )
        .await?;

    Ok(())
}

async fn seed_tables(client: &DinocoClient<dinoco_engine::SqliteAdapter>) -> DinocoResult<()> {
    client.primary().execute(r#"INSERT INTO "users" ("id", "name") VALUES (1, 'Alice'), (2, 'Bruno')"#, &[]).await?;

    client
        .primary()
        .execute(
            r#"INSERT INTO "posts" ("id", "title", "published", "authorId")
               VALUES
               (1, 'A1', 1, 1),
               (2, 'A2', 0, 1),
               (3, 'A3', 1, 1),
               (4, 'B1', 1, 2)"#,
            &[],
        )
        .await?;

    client
        .primary()
        .execute(
            r#"INSERT INTO "comments" ("id", "text", "flagged", "postId")
               VALUES
               (1, 'c1', 0, 1),
               (2, 'c2', 0, 1),
               (3, 'c3', 1, 1),
               (4, 'c4', 0, 2),
               (5, 'c5', 0, 3),
               (6, 'c6', 0, 4),
               (7, 'c7', 1, 4)"#,
            &[],
        )
        .await?;

    Ok(())
}

#[tokio::test]
async fn find_many_and_find_first_can_count_relations() -> DinocoResult<()> {
    let client = DinocoClient::<dinoco_engine::SqliteAdapter>::new(
        sqlite_url("nested"),
        vec![],
        dinoco::DinocoClientConfig::default(),
    )
    .await?;

    create_tables(&client).await?;
    seed_tables(&client).await?;

    let users = find_many::<User>()
        .select::<UserListItem>()
        .order_by(|x| x.id.asc())
        .count(|x| x.posts().cond(|post| post.published.eq(true)))
        .includes(|x| {
            x.posts()
                .select::<PostListItem>()
                .order_by(|post| post.id.asc())
                .count(|post| post.comments().cond(|comment| comment.flagged.eq(false)))
                .includes(|post| {
                    post.comments()
                        .cond(|comment| comment.flagged.eq(false))
                        .order_by(|comment| comment.id.asc())
                        .select::<CommentListItem>()
                })
        })
        .execute(&client)
        .await?;

    assert_eq!(users.len(), 2);
    assert_eq!(users[0].posts_count, 2);
    assert_eq!(users[1].posts_count, 1);
    assert_eq!(users[0].posts[0].comments_count, 2);
    assert_eq!(users[0].posts[1].comments_count, 1);
    assert_eq!(users[1].posts[0].comments_count, 1);
    assert_eq!(users[0].posts[0].comments.iter().map(|item| item.id).collect::<Vec<_>>(), vec![1, 2]);

    let first_user = find_first::<User>()
        .select::<UserListItem>()
        .cond(|x| x.id.eq(1_i64))
        .count(|x| x.posts().cond(|post| post.published.eq(true)))
        .includes(|x| {
            x.posts()
                .select::<PostListItem>()
                .order_by(|post| post.id.asc())
                .count(|post| post.comments().cond(|comment| comment.flagged.eq(false)))
        })
        .execute(&client)
        .await?
        .expect("first user should exist");

    assert_eq!(first_user.id, 1);
    assert_eq!(first_user.posts_count, 2);
    assert_eq!(first_user.posts[0].comments_count, 2);

    Ok(())
}

impl User {
    pub fn __dinoco_load_posts<'a, P, C, A>(
        item_keys: Vec<Option<i64>>,
        include: &'a dinoco::IncludeNode,
        client: &'a DinocoClient<A>,
        read_mode: dinoco::ReadMode,
        relation_field: impl Fn(&mut P) -> &mut Vec<C> + Copy + Send + 'a,
    ) -> IncludeLoaderFuture<'a, P>
    where
        A: DinocoAdapter,
        C: Projection<Post>,
    {
        Box::pin(async move {
            struct PostRow<C> {
                item: C,
                relation_key: i64,
            }

            impl<C> DinocoRow for PostRow<C>
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
            let mut statement = include
                .statement
                .clone()
                .unwrap_or_else(|| dinoco_engine::SelectStatement::new().from("posts").select(C::columns()));
            let mut select_columns = statement.select.clone();

            if select_columns.is_empty() {
                select_columns = C::columns().iter().map(|column| column.to_string()).collect::<Vec<_>>();
            }

            select_columns.push("authorId".to_string());
            statement.select = select_columns;
            statement.conditions.push(
                dinoco_engine::Expression::Column("authorId".to_string())
                    .in_values(keys.iter().cloned().map(IntoDinocoValue::into_dinoco_value).collect()),
            );

            let (sql, params) = dinoco_engine::QueryBuilder::build_select(adapter.dialect(), &statement);
            let rows = adapter.query_as::<PostRow<C>>(&sql, &params).await?;
            let relation_keys = rows.iter().map(|row| row.relation_key).collect::<Vec<_>>();
            let mut children = rows.into_iter().map(|row| row.item).collect::<Vec<_>>();

            C::load_includes(&mut children, &include.includes, client, read_mode).await?;
            C::load_counts(&mut children, &include.counts, client, read_mode).await?;

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

    pub fn __dinoco_count_posts<'a, P, A>(
        item_keys: Vec<Option<i64>>,
        count: &'a dinoco::CountNode,
        client: &'a DinocoClient<A>,
        _read_mode: dinoco::ReadMode,
        relation_field: impl Fn(&mut P) -> &mut usize + Copy + Send + 'a,
    ) -> IncludeLoaderFuture<'a, P>
    where
        A: DinocoAdapter,
    {
        Box::pin(async move {
            #[derive(Debug, Clone, Rowable)]
            struct CountRow {
                authorId: i64,
            }

            let keys = item_keys.iter().flatten().cloned().collect::<Vec<_>>();

            if keys.is_empty() {
                return Ok(Box::new(|_: &mut [P]| {}) as dinoco::IncludeApplier<'a, P>);
            }

            let adapter = client.read_adapter(false);
            let mut statement = count
                .statement
                .clone()
                .unwrap_or_else(|| dinoco_engine::SelectStatement::new().from("posts").select(&["authorId"]));
            statement.select = vec!["authorId".to_string()];
            statement.conditions.push(
                dinoco_engine::Expression::Column("authorId".to_string())
                    .in_values(keys.iter().cloned().map(IntoDinocoValue::into_dinoco_value).collect()),
            );

            let (sql, params) = dinoco_engine::QueryBuilder::build_select(adapter.dialect(), &statement);
            let rows = adapter.query_as::<CountRow>(&sql, &params).await?;
            let mut grouped: HashMap<i64, usize> = HashMap::new();

            for row in rows {
                *grouped.entry(row.authorId).or_default() += 1;
            }

            Ok(Box::new(move |items: &mut [P]| {
                for (item, key) in items.iter_mut().zip(item_keys.into_iter()) {
                    *relation_field(item) = key.and_then(|key| grouped.remove(&key)).unwrap_or(0);
                }
            }) as dinoco::IncludeApplier<'a, P>)
        })
    }
}

impl Post {
    pub fn __dinoco_load_comments<'a, P, C, A>(
        item_keys: Vec<Option<i64>>,
        include: &'a dinoco::IncludeNode,
        client: &'a DinocoClient<A>,
        read_mode: dinoco::ReadMode,
        relation_field: impl Fn(&mut P) -> &mut Vec<C> + Copy + Send + 'a,
    ) -> IncludeLoaderFuture<'a, P>
    where
        A: DinocoAdapter,
        C: Projection<Comment>,
    {
        Box::pin(async move {
            struct CommentRow<C> {
                item: C,
                relation_key: i64,
            }

            impl<C> DinocoRow for CommentRow<C>
            where
                C: Projection<Comment>,
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
            let mut statement = include
                .statement
                .clone()
                .unwrap_or_else(|| dinoco_engine::SelectStatement::new().from("comments").select(C::columns()));
            let mut select_columns = statement.select.clone();

            if select_columns.is_empty() {
                select_columns = C::columns().iter().map(|column| column.to_string()).collect::<Vec<_>>();
            }

            select_columns.push("postId".to_string());
            statement.select = select_columns;
            statement.conditions.push(
                dinoco_engine::Expression::Column("postId".to_string())
                    .in_values(keys.iter().cloned().map(IntoDinocoValue::into_dinoco_value).collect()),
            );

            let (sql, params) = dinoco_engine::QueryBuilder::build_select(adapter.dialect(), &statement);
            let rows = adapter.query_as::<CommentRow<C>>(&sql, &params).await?;
            let relation_keys = rows.iter().map(|row| row.relation_key).collect::<Vec<_>>();
            let mut children = rows.into_iter().map(|row| row.item).collect::<Vec<_>>();

            C::load_includes(&mut children, &include.includes, client, read_mode).await?;
            C::load_counts(&mut children, &include.counts, client, read_mode).await?;

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

    pub fn __dinoco_count_comments<'a, P, A>(
        item_keys: Vec<Option<i64>>,
        count: &'a dinoco::CountNode,
        client: &'a DinocoClient<A>,
        _read_mode: dinoco::ReadMode,
        relation_field: impl Fn(&mut P) -> &mut usize + Copy + Send + 'a,
    ) -> IncludeLoaderFuture<'a, P>
    where
        A: DinocoAdapter,
    {
        Box::pin(async move {
            #[derive(Debug, Clone, Rowable)]
            struct CountRow {
                postId: i64,
            }

            let keys = item_keys.iter().flatten().cloned().collect::<Vec<_>>();

            if keys.is_empty() {
                return Ok(Box::new(|_: &mut [P]| {}) as dinoco::IncludeApplier<'a, P>);
            }

            let adapter = client.read_adapter(false);
            let mut statement = count
                .statement
                .clone()
                .unwrap_or_else(|| dinoco_engine::SelectStatement::new().from("comments").select(&["postId"]));
            statement.select = vec!["postId".to_string()];
            statement.conditions.push(
                dinoco_engine::Expression::Column("postId".to_string())
                    .in_values(keys.iter().cloned().map(IntoDinocoValue::into_dinoco_value).collect()),
            );

            let (sql, params) = dinoco_engine::QueryBuilder::build_select(adapter.dialect(), &statement);
            let rows = adapter.query_as::<CountRow>(&sql, &params).await?;
            let mut grouped: HashMap<i64, usize> = HashMap::new();

            for row in rows {
                *grouped.entry(row.postId).or_default() += 1;
            }

            Ok(Box::new(move |items: &mut [P]| {
                for (item, key) in items.iter_mut().zip(item_keys.into_iter()) {
                    *relation_field(item) = key.and_then(|key| grouped.remove(&key)).unwrap_or(0);
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
        &["id", "title", "published", "authorId"]
    }
}

impl Projection<Comment> for Comment {
    fn columns() -> &'static [&'static str] {
        &["id", "text", "flagged", "postId"]
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
        "posts"
    }
}

impl Model for Comment {
    type Include = CommentInclude;
    type Where = CommentWhere;

    fn table_name() -> &'static str {
        "comments"
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
            title: ScalarField::new("title"),
            published: ScalarField::new("published"),
            authorId: ScalarField::new("authorId"),
        }
    }
}

impl Default for CommentWhere {
    fn default() -> Self {
        Self {
            id: ScalarField::new("id"),
            text: ScalarField::new("text"),
            flagged: ScalarField::new("flagged"),
            postId: ScalarField::new("postId"),
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

impl Default for CommentInclude {
    fn default() -> Self {
        Self {}
    }
}

impl UserInclude {
    fn posts(&self) -> RelationField<Post> {
        RelationField::new("posts")
    }
}

impl PostInclude {
    fn comments(&self) -> RelationField<Comment> {
        RelationField::new("comments")
    }
}
