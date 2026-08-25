use dinoco::{AtomicUpdateError, DinocoEnum, Entity, find_first, find_many, insert_into, insert_many};
use dinoco_engine::{Backend, DinocoAdapter, DinocoClient, MigrationColumnType, MigrationDefault, SqliteAdapter};
use dinoco_tests::{column, create_table, default, nullable, primary};

#[derive(Debug, Entity)]
#[dinoco(table_name = "user")]
pub struct User {
    #[dinoco(auto_generate = uuid)]
    id: String,
    email: String,
    office: String,

    #[dinoco(one_to_many, foreign_key = "user_id", references = "id")]
    tokens: Vec<UserToken>,
}

#[derive(Debug, Entity)]
#[dinoco(table_name = "user_token")]
pub struct UserToken {
    #[dinoco(auto_generate = uuid)]
    id: String,
    #[dinoco(default = false)]
    is_expired: bool,
    user_id: Option<String>,

    #[dinoco(one_to_many, foreign_key = "user_id", references = "id")]
    user: Option<User>,
}

#[derive(Debug, Entity)]
#[dinoco(table_name = "user_post")]
pub struct UserPost {
    user_id: String,
    post_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, DinocoEnum)]
enum OptionalStatus {
    #[default]
    Active,
    Disabled,
}

#[derive(Debug, Entity)]
#[dinoco(table_name = "numeric_account")]
struct NumericAccount {
    #[dinoco(primary_key)]
    id: String,
    balance: i64,
    total: i64,
    counter: i64,
    multiplier: f64,
    optional_value: Option<i64>,
    #[dinoco(enum)]
    status: Option<OptionalStatus>,
    #[dinoco(enum, default = Option::Some(OptionalStatus::Active))]
    default_status: Option<OptionalStatus>,
}

#[tokio::test]
async fn sqlite_crud_relations_and_count_work_end_to_end() -> anyhow::Result<()> {
    let path = format!("/private/tmp/dinoco-test-{}.sqlite", std::process::id());
    let adapter = SqliteAdapter::new(path.clone()).await.map_err(anyhow::Error::msg)?;
    create_table(
        &adapter,
        "user",
        vec![
            primary(column("id", MigrationColumnType::String)),
            column("email", MigrationColumnType::String),
            column("office", MigrationColumnType::String),
        ],
    )
    .await?;
    create_table(
        &adapter,
        "user_token",
        vec![
            primary(column("id", MigrationColumnType::String)),
            default(column("is_expired", MigrationColumnType::Boolean), MigrationDefault::Boolean(false)),
            nullable(column("user_id", MigrationColumnType::String)),
        ],
    )
    .await?;
    create_table(
        &adapter,
        "user_post",
        vec![column("user_id", MigrationColumnType::String), column("post_id", MigrationColumnType::String)],
    )
    .await?;

    let client = DinocoClient::new(Backend::Sqlite(adapter));

    let mut user = User::new("a@dinoco.rs".to_string(), "admin".to_string());
    user.tokens = vec![UserToken::new()];
    insert_into::<User>().values(&user).execute(&client).await?;

    let users = find_many::<User>()
        .includes(|x| {
            x.tokens()
                .where_(|token| token.is_expired.eq(true))
                .where_complex(|token, m| m.or(token.is_expired.eq(false), token.id.eq("missing")))
                .where_(|token| token.is_expired.eq(true))
        })
        .execute(&client)
        .await?;
    assert_eq!(users.len(), 1);
    assert_eq!(users[0].tokens.len(), 1);

    dinoco::update::<User>()
        .where_(|x| x.email.eq("a@dinoco.rs"))
        .update(|x| x.email.set("b@dinoco.rs".to_string()))
        .execute(&client)
        .await?;

    let updated =
        find_first::<User>().where_(|x| x.email.eq("b@dinoco.rs")).execute(&client).await?.expect("updated user");
    assert_eq!(updated.email, "b@dinoco.rs");

    let updated = dinoco::find_and_update::<User>()
        .where_(|x| x.email.eq("b@dinoco.rs"))
        .update(|x| x.office.set("member".to_string()))
        .execute(&client)
        .await?;
    assert_eq!(updated.office, "member");

    let missing = dinoco::find_and_update::<User>()
        .where_(|x| x.email.eq("missing@dinoco.rs"))
        .update(|x| x.office.set("member".to_string()))
        .execute(&client)
        .await
        .expect_err("find_and_update must fail when no row is affected");
    assert!(matches!(missing, AtomicUpdateError::RowNotAffected));

    let count = dinoco::count::<User>().includes(|x| x.tokens()).execute(&client).await?;
    assert_eq!(count.total, 1);
    assert_eq!(count.tokens, Some(1));

    dinoco::delete_many::<UserToken>().where_(|x| x.user_id.not_null()).execute(&client).await?;
    let count = dinoco::count::<UserToken>().execute(&client).await?;
    assert_eq!(count.total, 0);

    let many_a = User::new("many-a@dinoco.rs".to_string(), "admin".to_string());
    let many_b = User::new("many-b@dinoco.rs".to_string(), "member".to_string());
    insert_many::<User>().values(vec![many_a, many_b]).execute(&client).await?;

    dinoco::update_many::<User>()
        .where_(|x| x.office.eq("admin"))
        .update(|x| x.office.set("owner".to_string()))
        .execute(&client)
        .await?;
    let owners = find_many::<User>().where_(|x| x.office.eq("owner")).execute(&client).await?;
    assert_eq!(owners.len(), 1);

    dinoco::update::<UserPost>()
        .where_(|x| x.user_id.eq("user-a"))
        .update(|x| x.post_id.connect("post-a"))
        .execute(&client)
        .await?;
    let pivot_count = adapter_count(&client, "SELECT COUNT(*) FROM user_post").await?;
    assert_eq!(pivot_count, 1);

    dinoco::update_many::<UserPost>()
        .where_(|x| x.user_id.eq("user-a"))
        .update(|x| x.post_id.disconnect("post-a"))
        .execute(&client)
        .await?;
    let pivot_count = adapter_count(&client, "SELECT COUNT(*) FROM user_post").await?;
    assert_eq!(pivot_count, 0);

    dinoco::delete::<User>().where_(|x| x.email.eq("many-b@dinoco.rs")).execute(&client).await?;
    let deleted =
        find_first::<User>().where_(|x| x.email.eq("many-b@dinoco.rs")).read_in_primary().execute(&client).await?;
    assert!(deleted.is_none());

    let _ = std::fs::remove_file(path);
    Ok(())
}

#[tokio::test]
async fn replicas_serve_finds_while_primary_reads_and_find_and_update_stay_on_primary() -> anyhow::Result<()> {
    let suffix = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_nanos();
    let primary_path = format!("/private/tmp/dinoco-primary-{}-{suffix}.sqlite", std::process::id());
    let replica_path = format!("/private/tmp/dinoco-replica-{}-{suffix}.sqlite", std::process::id());
    let primary_adapter = SqliteAdapter::new(primary_path.clone()).await.map_err(anyhow::Error::msg)?;
    let replica_adapter = SqliteAdapter::new(replica_path.clone()).await.map_err(anyhow::Error::msg)?;

    for adapter in [&primary_adapter, &replica_adapter] {
        create_table(
            adapter,
            "user",
            vec![
                primary(column("id", MigrationColumnType::String)),
                column("email", MigrationColumnType::String),
                column("office", MigrationColumnType::String),
            ],
        )
        .await?;
    }
    primary_adapter
        .execute(
            "INSERT INTO user (id, email, office) VALUES (?, ?, ?)",
            &["primary-id".into(), "primary@dinoco.rs".into(), "primary-before".into()],
        )
        .await?;
    replica_adapter
        .execute(
            "INSERT INTO user (id, email, office) VALUES (?, ?, ?)",
            &["replica-id".into(), "replica@dinoco.rs".into(), "replica-value".into()],
        )
        .await?;

    let client =
        DinocoClient::new(Backend::Sqlite(primary_adapter)).with_read_replicas(vec![Backend::Sqlite(replica_adapter)]);

    let regular = find_first::<User>().execute(&client).await?.expect("replica row");
    assert_eq!(regular.email, "replica@dinoco.rs");

    let primary_read = find_first::<User>().read_in_primary().execute(&client).await?.expect("primary row");
    assert_eq!(primary_read.email, "primary@dinoco.rs");

    let updated = dinoco::find_and_update::<User>()
        .where_(|x| x.email.eq("primary@dinoco.rs"))
        .update(|x| x.office.set("primary-after".to_string()))
        .execute(&client)
        .await?;
    assert_eq!(updated.office, "primary-after");

    let replica_after = find_first::<User>().execute(&client).await?.expect("unchanged replica row");
    assert_eq!(replica_after.office, "replica-value");
    let primary_after = find_first::<User>().read_in_primary().execute(&client).await?.expect("updated primary row");
    assert_eq!(primary_after.office, "primary-after");

    drop(client);
    let _ = std::fs::remove_file(primary_path);
    let _ = std::fs::remove_file(replica_path);
    Ok(())
}

async fn adapter_count(client: &DinocoClient, sql: &str) -> anyhow::Result<i64> {
    match &client.backend {
        Backend::Sqlite(adapter) => adapter.query_count(sql, &[]).await,
        _ => unreachable!("sqlite test"),
    }
}

#[tokio::test]
async fn atomic_numeric_updates_optional_enum_and_concurrency_work() -> anyhow::Result<()> {
    let suffix = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_nanos();
    let path = format!("/private/tmp/dinoco-atomic-{}-{suffix}.sqlite", std::process::id());
    let adapter = SqliteAdapter::new(path.clone()).await.map_err(anyhow::Error::msg)?;
    create_table(
        &adapter,
        "numeric_account",
        vec![
            primary(column("id", MigrationColumnType::String)),
            column("balance", MigrationColumnType::Integer),
            column("total", MigrationColumnType::Integer),
            column("counter", MigrationColumnType::Integer),
            column("multiplier", MigrationColumnType::Float),
            nullable(column("optional_value", MigrationColumnType::Integer)),
            nullable(column("status", MigrationColumnType::String)),
            nullable(column("default_status", MigrationColumnType::String)),
        ],
    )
    .await?;
    let client = DinocoClient::new(Backend::Sqlite(adapter));
    let account = NumericAccount::new("account-1".to_string(), 100, 10, 4, 8.0);
    insert_into::<NumericAccount>().values(&account).execute(&client).await?;

    let updated = dinoco::find_and_update::<NumericAccount>()
        .where_(|item| item.id.eq("account-1"))
        .update(|item| item.balance.increment(50))
        .update(|item| item.total.decrement(2))
        .update(|item| item.counter.multiply(4))
        .update(|item| item.multiplier.divide(2.0))
        .execute(&client)
        .await?;
    assert_eq!(updated.balance, 150);
    assert_eq!(updated.total, 8);
    assert_eq!(updated.counter, 16);
    assert_eq!(updated.multiplier, 4.0);
    assert!(updated.optional_value.is_none());
    assert!(updated.status.is_none());
    assert_eq!(updated.default_status, Some(OptionalStatus::Active));

    let active = find_first::<NumericAccount>()
        .where_(|item| item.default_status.eq(OptionalStatus::Active))
        .read_in_primary()
        .execute(&client)
        .await?
        .expect("optional enum default should be filterable");
    assert_eq!(active.id, "account-1");

    let updated = dinoco::find_and_update::<NumericAccount>()
        .where_(|item| item.id.eq("account-1"))
        .update(|item| item.status.set(Some(OptionalStatus::Disabled)))
        .update(|item| item.optional_value.increment(5))
        .update(|item| item.counter.divide(4))
        .update(|item| item.multiplier.multiply(1.5))
        .execute(&client)
        .await?;
    assert_eq!(updated.status, Some(OptionalStatus::Disabled));
    assert_eq!(updated.counter, 4);
    assert_eq!(updated.multiplier, 6.0);
    // SQL NULL arithmetic follows database semantics; Dinoco does not inject COALESCE.
    assert!(updated.optional_value.is_none());

    let updated = dinoco::find_and_update::<NumericAccount>()
        .where_(|item| item.id.eq("account-1"))
        .update(|item| item.multiplier.increment(1.0))
        .execute(&client)
        .await?;
    assert_eq!(updated.multiplier, 7.0);

    let updated = dinoco::find_and_update::<NumericAccount>()
        .where_(|item| item.id.eq("account-1"))
        .update(|item| item.multiplier.decrement(0.5))
        .update(|item| item.status.set(None::<OptionalStatus>))
        .execute(&client)
        .await?;
    assert_eq!(updated.multiplier, 6.5);
    assert!(updated.status.is_none());

    let duplicate = dinoco::find_and_update::<NumericAccount>()
        .where_(|item| item.id.eq("account-1"))
        .update(|item| item.balance.increment(1))
        .update(|item| item.balance.decrement(1))
        .execute(&client)
        .await;
    assert!(matches!(duplicate, Err(AtomicUpdateError::DuplicateField("balance"))));

    let missing = dinoco::find_and_update::<NumericAccount>()
        .where_(|item| item.id.eq("missing"))
        .update(|item| item.counter.increment(1))
        .execute(&client)
        .await;
    assert!(matches!(missing, Err(AtomicUpdateError::RowNotAffected)));

    dinoco::find_and_update::<NumericAccount>()
        .where_(|item| item.id.eq("account-1"))
        .update(|item| item.balance.set(100))
        .execute(&client)
        .await?;
    let first = dinoco::find_and_update::<NumericAccount>()
        .where_(|item| item.id.eq("account-1"))
        .where_(|item| item.balance.gte(80))
        .update(|item| item.balance.decrement(80))
        .execute(&client);
    let second = dinoco::find_and_update::<NumericAccount>()
        .where_(|item| item.id.eq("account-1"))
        .where_(|item| item.balance.gte(80))
        .update(|item| item.balance.decrement(80))
        .execute(&client);
    let (first, second) = tokio::join!(first, second);
    assert_eq!(usize::from(first.is_ok()) + usize::from(second.is_ok()), 1);
    assert_eq!(
        usize::from(matches!(first, Err(AtomicUpdateError::RowNotAffected)))
            + usize::from(matches!(second, Err(AtomicUpdateError::RowNotAffected))),
        1
    );
    let loaded = find_first::<NumericAccount>()
        .where_(|item| item.id.eq("account-1"))
        .read_in_primary()
        .execute(&client)
        .await?
        .expect("account");
    assert_eq!(loaded.balance, 20);

    drop(client);
    let _ = std::fs::remove_file(path);
    Ok(())
}
