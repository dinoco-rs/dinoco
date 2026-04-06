#![allow(non_snake_case)]

#[path = "../dinoco/mod.rs"]
mod app;

use std::env;

use app::models::User;
use dinoco::{DinocoAdapter, DinocoClient, DinocoResult, Extend, find_many};
use uuid::Uuid;

#[derive(Debug, Clone, Extend)]
#[extend(app::models::Post)]
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
    let client = DinocoClient::<dinoco_engine::SqliteAdapter>::new(sqlite_url("per-parent"), vec![]).await?;

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
