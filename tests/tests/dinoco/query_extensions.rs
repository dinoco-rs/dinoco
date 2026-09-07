use dinoco::{Entity, delete, delete_many, exists, find_batch, find_first, find_many, insert_into, insert_many, update};
use dinoco_engine::serde_json::json;
use dinoco_engine::{
    Backend, DinocoAdapter, DinocoClient, MigrationColumnType, PostgresAdapter, QueryMode, SqliteAdapter,
};
use dinoco_tests::{column, create_table, drop_table, primary};

const POSTGRES_URL: &str = "postgres://postgres:postgres@localhost:5432/postgres";

#[derive(Debug, Entity)]
#[dinoco(table_name = "qx_user")]
pub struct User {
    #[dinoco(primary_key)]
    id: String,
    name: String,
    age: i64,

    #[dinoco(
        many_to_many,
        foreign_key = "id",
        references = "id",
        join_table = "_qx_user_to_qx_tag",
        parent_field = "id",
        join_parent_field = "user_id",
        join_child_field = "tag_id"
    )]
    tags: Vec<Tag>,

    #[dinoco(
        many_to_many_key,
        join_table = "_qx_user_to_qx_tag",
        parent_field = "id",
        join_parent_field = "user_id",
        join_child_field = "tag_id"
    )]
    tag_id: Option<String>,
}

#[derive(Debug, Entity)]
#[dinoco(table_name = "qx_tag")]
pub struct Tag {
    #[dinoco(primary_key)]
    id: String,
    name: String,
}

#[derive(Debug, Entity)]
#[dinoco(table_name = "qx_post")]
pub struct Post {
    #[dinoco(primary_key)]
    id: String,
    title: String,
}

async fn client(name: &str) -> anyhow::Result<(DinocoClient, String)> {
    let path = format!("/private/tmp/dinoco-{name}-{}-{}.sqlite", std::process::id(), monotonic());
    let adapter = SqliteAdapter::new(path.clone()).await.map_err(anyhow::Error::msg)?;
    Ok((DinocoClient::new(Backend::Sqlite(adapter)), path))
}

fn monotonic() -> u128 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
}

async fn setup(name: &str) -> anyhow::Result<(DinocoClient, String)> {
    let (client, path) = client(name).await?;
    let Backend::Sqlite(adapter) = &client.backend else { unreachable!("sqlite test") };

    create_table(
        adapter,
        "qx_user",
        vec![
            primary(column("id", MigrationColumnType::String)),
            column("name", MigrationColumnType::String),
            column("age", MigrationColumnType::Integer),
        ],
    )
    .await?;
    create_table(adapter, "qx_tag", vec![primary(column("id", MigrationColumnType::String)), column("name", MigrationColumnType::String)])
        .await?;
    create_table(
        adapter,
        "qx_post",
        vec![primary(column("id", MigrationColumnType::String)), column("title", MigrationColumnType::String)],
    )
    .await?;
    create_table(
        adapter,
        "_qx_user_to_qx_tag",
        vec![column("user_id", MigrationColumnType::String), column("tag_id", MigrationColumnType::String)],
    )
    .await?;

    Ok((client, path))
}

#[tokio::test]
async fn exists_reports_whether_a_matching_row_exists() -> anyhow::Result<()> {
    let (client, path) = setup("exists").await?;

    insert_into::<User>().values(User::new("user-1".to_string(), "Ada".to_string(), 30)).execute(&client).await?;

    assert!(exists::<User>().where_(|x| x.name.eq("Ada")).execute(&client).await?);
    assert!(!exists::<User>().where_(|x| x.name.eq("Nobody")).execute(&client).await?);

    let _ = std::fs::remove_file(path);
    Ok(())
}

#[tokio::test]
async fn pluck_projects_a_single_column_instead_of_a_full_row() -> anyhow::Result<()> {
    let (client, path) = setup("pluck-find").await?;

    insert_many::<User>()
        .values(vec![
            User::new("user-1".to_string(), "Ada".to_string(), 30),
            User::new("user-2".to_string(), "Grace".to_string(), 40),
        ])
        .execute(&client)
        .await?;

    let mut ids = find_many::<User>().order_by(|x| x.id.asc()).pluck(|x| x.id).execute(&client).await?;
    ids.sort();
    assert_eq!(ids, vec!["user-1".to_string(), "user-2".to_string()]);

    let age = find_first::<User>().where_(|x| x.name.eq("Grace")).pluck(|x| x.age).execute(&client).await?;
    assert_eq!(age, Some(40));

    let missing = find_first::<User>().where_(|x| x.name.eq("Nobody")).pluck(|x| x.age).execute(&client).await?;
    assert_eq!(missing, None);

    let _ = std::fs::remove_file(path);
    Ok(())
}

#[tokio::test]
async fn transform_maps_rows_after_the_query_runs() -> anyhow::Result<()> {
    let (client, path) = setup("transform-find").await?;

    insert_into::<User>().values(User::new("user-1".to_string(), "Ada".to_string(), 30)).execute(&client).await?;

    let rows = find_many::<User>()
        .transform(|user| json!({ "id": user.id, "name": user.name }))
        .execute(&client)
        .await?;
    assert_eq!(rows, vec![json!({ "id": "user-1", "name": "Ada" })]);

    let row = find_first::<User>()
        .where_(|x| x.name.eq("Ada"))
        .transform(|user| json!({ "id": user.id, "name": user.name }))
        .execute(&client)
        .await?;
    assert_eq!(row, Some(json!({ "id": "user-1", "name": "Ada" })));

    let _ = std::fs::remove_file(path);
    Ok(())
}

#[tokio::test]
async fn connect_batch_and_disconnect_batch_link_many_values_in_one_round_trip() -> anyhow::Result<()> {
    let (client, path) = setup("connect-batch").await?;

    insert_into::<User>().values(User::new("user-1".to_string(), "Ada".to_string(), 30)).execute(&client).await?;
    insert_many::<Tag>()
        .values(vec![
            Tag::new("tag-1".to_string(), "rust".to_string()),
            Tag::new("tag-2".to_string(), "db".to_string()),
            Tag::new("tag-3".to_string(), "orm".to_string()),
        ])
        .execute(&client)
        .await?;

    update::<User>()
        .where_(|x| x.id.eq("user-1"))
        .update(|x| x.tag_id.connect_batch(vec!["tag-1", "tag-2", "tag-3"]))
        .execute(&client)
        .await?;

    let loaded = find_first::<User>().where_(|x| x.id.eq("user-1")).includes(|x| x.tags()).execute(&client).await?.expect("user");
    assert_eq!(loaded.tags.len(), 3);

    update::<User>()
        .where_(|x| x.id.eq("user-1"))
        .update(|x| x.tag_id.disconnect_batch(vec!["tag-1", "tag-2"]))
        .execute(&client)
        .await?;

    let loaded = find_first::<User>().where_(|x| x.id.eq("user-1")).includes(|x| x.tags()).execute(&client).await?.expect("user");
    assert_eq!(loaded.tags.len(), 1);
    assert_eq!(loaded.tags[0].id, "tag-3");

    let _ = std::fs::remove_file(path);
    Ok(())
}

#[tokio::test]
async fn find_batch_runs_independent_queries_together_in_batch_query_mode() -> anyhow::Result<()> {
    let (client, path) = setup("find-batch-multi").await?;

    insert_many::<User>()
        .values(vec![
            User::new("user-1".to_string(), "Ada".to_string(), 30),
            User::new("user-2".to_string(), "Grace".to_string(), 40),
        ])
        .execute(&client)
        .await?;
    insert_into::<Tag>().values(Tag::new("tag-1".to_string(), "rust".to_string())).execute(&client).await?;
    insert_into::<Post>().values(Post::new("post-1".to_string(), "Hello".to_string())).execute(&client).await?;

    let (users, tags, posts) =
        find_batch((find_many::<User>(), find_many::<Tag>(), find_many::<Post>())).execute(&client).await?;

    assert_eq!(users.len(), 2);
    assert_eq!(tags.len(), 1);
    assert_eq!(posts.len(), 1);
    assert_eq!(posts[0].title, "Hello");

    let _ = std::fs::remove_file(path);
    Ok(())
}

#[tokio::test]
async fn find_batch_runs_as_a_single_round_trip_in_single_query_mode() -> anyhow::Result<()> {
    let (client, path) = setup("find-batch-single").await?;
    let client = client.with_query_mode(QueryMode::SingleQuery);

    insert_many::<User>()
        .values(vec![
            User::new("user-1".to_string(), "Ada".to_string(), 30),
            User::new("user-2".to_string(), "Grace".to_string(), 40),
        ])
        .execute(&client)
        .await?;
    insert_into::<Tag>().values(Tag::new("tag-1".to_string(), "rust".to_string())).execute(&client).await?;

    let (mut users, tags, grace) = find_batch((
        find_many::<User>(),
        find_many::<Tag>(),
        find_first::<User>().where_(|x| x.name.eq("Grace")),
    ))
    .execute(&client)
    .await?;
    users.sort_by(|a, b| a.id.cmp(&b.id));

    assert_eq!(users.len(), 2);
    assert_eq!(users[0].name, "Ada");
    assert_eq!(users[1].age, 40);
    assert_eq!(tags.len(), 1);
    assert_eq!(tags[0].name, "rust");
    assert_eq!(grace.map(|user| user.age), Some(40));

    let _ = std::fs::remove_file(path);
    Ok(())
}

#[tokio::test]
async fn pluck_and_transform_work_on_insert_and_update_returning() -> anyhow::Result<()> {
    let (client, path) = setup("returning-extensions").await?;

    let id = insert_into::<User>().values(User::new("user-1".to_string(), "Ada".to_string(), 30)).pluck(|x| x.id).execute(&client).await?;
    assert_eq!(id, "user-1");

    let name = insert_into::<User>()
        .values(User::new("user-2".to_string(), "Grace".to_string(), 40))
        .returning::<User>()
        .transform(|user| user.name)
        .execute(&client)
        .await?;
    assert_eq!(name, "Grace");

    let ages = update::<User>()
        .where_(|x| x.id.eq("user-1"))
        .update(|x| x.age.set(31))
        .pluck(|x| x.age)
        .execute(&client)
        .await?;
    assert_eq!(ages, vec![31]);

    let names = update::<User>()
        .where_(|x| x.id.eq("user-1"))
        .update(|x| x.age.set(32))
        .returning::<User>()
        .transform(|user| user.age)
        .execute(&client)
        .await?;
    assert_eq!(names, vec![32]);

    let _ = std::fs::remove_file(path);
    Ok(())
}

#[derive(Debug, Entity)]
#[dinoco(table_name = "qxpg_user_exists")]
pub struct PgExistsUser {
    #[dinoco(primary_key)]
    id: String,
    name: String,
    age: i64,
}

#[tokio::test]
async fn exists_and_find_batch_single_query_mode_work_on_postgres() -> anyhow::Result<()> {
    let adapter = PostgresAdapter::direct(POSTGRES_URL).await?;
    drop_table(&adapter, "qxpg_user_exists").await?;
    create_table(
        &adapter,
        "qxpg_user_exists",
        vec![
            primary(column("id", MigrationColumnType::String)),
            column("name", MigrationColumnType::String),
            column("age", MigrationColumnType::Integer),
        ],
    )
    .await?;
    let client = DinocoClient::new(Backend::Postgres(adapter)).with_query_mode(QueryMode::SingleQuery);

    insert_many::<PgExistsUser>()
        .values(vec![
            PgExistsUser::new("pg-user-1".to_string(), "Ada".to_string(), 30),
            PgExistsUser::new("pg-user-2".to_string(), "Grace".to_string(), 40),
        ])
        .execute(&client)
        .await?;

    assert!(exists::<PgExistsUser>().where_(|x| x.name.eq("Ada")).execute(&client).await?);
    assert!(!exists::<PgExistsUser>().where_(|x| x.name.eq("Nobody")).execute(&client).await?);

    let ids = find_many::<PgExistsUser>().order_by(|x| x.id.asc()).pluck(|x| x.id).execute(&client).await?;
    assert_eq!(ids, vec!["pg-user-1".to_string(), "pg-user-2".to_string()]);

    let (mut users, grace) = find_batch((
        find_many::<PgExistsUser>(),
        find_first::<PgExistsUser>().where_(|x| x.name.eq("Grace")),
    ))
    .execute(&client)
    .await?;
    users.sort_by(|a, b| a.id.cmp(&b.id));

    assert_eq!(users.len(), 2);
    assert_eq!(users[0].name, "Ada");
    assert_eq!(grace.map(|user| user.age), Some(40));

    Ok(())
}

#[tokio::test]
async fn pluck_and_transform_work_on_delete_and_delete_many_returning() -> anyhow::Result<()> {
    let (client, path) = setup("delete-extensions").await?;

    insert_many::<User>()
        .values(vec![
            User::new("user-1".to_string(), "Ada".to_string(), 30),
            User::new("user-2".to_string(), "Grace".to_string(), 40),
            User::new("user-3".to_string(), "Linus".to_string(), 50),
        ])
        .execute(&client)
        .await?;

    let deleted_id = delete::<User>().where_(|x| x.id.eq("user-1")).pluck(|x| x.id).execute(&client).await?;
    assert_eq!(deleted_id, vec!["user-1".to_string()]);
    assert!(!exists::<User>().where_(|x| x.id.eq("user-1")).execute(&client).await?);

    let deleted_name = delete::<User>()
        .where_(|x| x.id.eq("user-2"))
        .returning::<User>()
        .transform(|user| user.name)
        .execute(&client)
        .await?;
    assert_eq!(deleted_name, vec!["Grace".to_string()]);

    let deleted_ids = delete_many::<User>().where_(|x| x.age.gte(0)).pluck(|x| x.id).execute(&client).await?;
    assert_eq!(deleted_ids, vec!["user-3".to_string()]);
    assert_eq!(exists::<User>().execute(&client).await?, false);

    let _ = std::fs::remove_file(path);
    Ok(())
}
