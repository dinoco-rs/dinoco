//! Behaviour the runtime rewrite must keep (or fixed): decoding rows of
//! hand-written statements, owned inserts, batched relation writes and enum
//! columns.

use std::sync::{Arc, Mutex};

use dinoco::chrono::{DateTime, NaiveDate, TimeZone, Utc};
use dinoco::{
    DinocoEnum, Entity, find_first, find_many, insert_into, insert_many, serde_json, setup_test_methods, update_many,
};
use dinoco_engine::{Backend, DinocoAdapter, DinocoClient, SqliteAdapter};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, DinocoEnum)]
enum Plan {
    #[default]
    #[dinoco(value = "free")]
    Free,
    #[dinoco(value = "pro-annual")]
    ProAnnual,
}

#[derive(Debug, Clone, PartialEq, Entity)]
#[dinoco(table_name = "rt_account")]
pub struct Account {
    #[dinoco(primary_key, auto_generate = uuid)]
    id: String,
    email: String,
    nickname: Option<String>,
    avatar: Vec<u8>,
    settings: serde_json::Value,
    visits: i64,
    ratio: f64,
    active: bool,
    birthday: NaiveDate,
    created_at: DateTime<Utc>,
    deleted_at: Option<DateTime<Utc>>,
    #[dinoco(enum)]
    plan: Plan,
}

#[derive(Debug, Clone, Entity)]
#[dinoco(table_name = "rt_user")]
pub struct User {
    #[dinoco(primary_key)]
    id: String,
    name: String,

    #[dinoco(
        many_to_many,
        foreign_key = "id",
        references = "id",
        join_table = "_rt_user_to_rt_tag",
        parent_field = "id",
        join_parent_field = "user_id",
        join_child_field = "tag_id"
    )]
    tags: Vec<Tag>,

    #[dinoco(
        many_to_many_key,
        join_table = "_rt_user_to_rt_tag",
        parent_field = "id",
        join_parent_field = "user_id",
        join_child_field = "tag_id"
    )]
    tag_id: Option<String>,
}

#[derive(Debug, Clone, Entity)]
#[dinoco(table_name = "rt_tag")]
pub struct Tag {
    #[dinoco(primary_key)]
    id: String,
    name: String,
}

/// The join table, modelled only to observe the statements run on it.
#[derive(Debug, Clone, Entity)]
#[dinoco(table_name = "_rt_user_to_rt_tag")]
pub struct UserTag {
    user_id: String,
    tag_id: String,
}

async fn client() -> anyhow::Result<(DinocoClient, SqliteAdapter)> {
    let adapter = SqliteAdapter::memory().await?;
    for statement in [
        // Columns deliberately not in the struct's field order.
        "CREATE TABLE rt_account (created_at TEXT NOT NULL, plan TEXT NOT NULL, id TEXT PRIMARY KEY, \
         deleted_at TEXT, email TEXT NOT NULL, nickname TEXT, avatar BLOB NOT NULL, settings BLOB NOT NULL, \
         visits INTEGER NOT NULL, ratio REAL NOT NULL, active BOOLEAN NOT NULL, birthday TEXT NOT NULL)",
        "CREATE TABLE rt_user (id TEXT PRIMARY KEY, name TEXT NOT NULL)",
        "CREATE TABLE rt_tag (id TEXT PRIMARY KEY, name TEXT NOT NULL)",
        "CREATE TABLE _rt_user_to_rt_tag (user_id TEXT NOT NULL, tag_id TEXT NOT NULL)",
    ] {
        adapter.execute(statement, &[]).await?;
    }

    Ok((DinocoClient::new(Backend::Sqlite(adapter.clone())), adapter))
}

fn account(index: u8) -> Account {
    Account {
        id: format!("account-{index}"),
        email: format!("user{index}@dinoco.rs"),
        nickname: index.is_multiple_of(2).then(|| format!("nick-{index}")),
        avatar: vec![index, 0, 255],
        settings: serde_json::json!({ "theme": "dark", "index": index }),
        visits: i64::from(index) * 10,
        ratio: f64::from(index) / 4.0,
        active: !index.is_multiple_of(2),
        birthday: NaiveDate::from_ymd_opt(1990, 1, u32::from(index) + 1).unwrap(),
        created_at: Utc.timestamp_opt(1_700_000_000 + i64::from(index), 0).unwrap(),
        deleted_at: index.is_multiple_of(2).then(|| Utc.timestamp_opt(1_800_000_000, 0).unwrap()),
        plan: if index.is_multiple_of(2) { Plan::ProAnnual } else { Plan::Free },
    }
}

#[tokio::test]
async fn rows_decode_whatever_the_column_order() -> anyhow::Result<()> {
    let (client, adapter) = client().await?;
    let expected = vec![account(1), account(2)];
    insert_many::<Account>().values(expected.clone()).execute(&client).await?;

    // Dinoco's own projection, in field order.
    let mut found = find_many::<Account>().order_by(|x| x.id.asc()).execute(&client).await?;
    assert_eq!(found, expected);

    // Hand-written statements: table order, and an arbitrary order with a
    // differently cased column name.
    found = adapter.query::<Account>("SELECT * FROM rt_account ORDER BY id", &[]).await?;
    assert_eq!(found, expected);
    found = adapter
        .query::<Account>(
            "SELECT plan, birthday, ACTIVE, ratio, visits, settings, avatar, nickname, email, id, deleted_at, \
             created_at FROM rt_account ORDER BY id",
            &[],
        )
        .await?;
    assert_eq!(found, expected);

    // A missing column fails the row instead of reading another one.
    assert!(adapter.query::<Account>("SELECT id, email FROM rt_account", &[]).await.is_err());
    Ok(())
}

#[tokio::test]
async fn owned_and_borrowed_inserts_store_the_same_values() -> anyhow::Result<()> {
    let (client, _) = client().await?;
    let owned = account(3);
    let borrowed = account(4);

    insert_into::<Account>().values(owned.clone()).execute(&client).await?;
    insert_into::<Account>().values(&borrowed).execute(&client).await?;
    let returned = insert_into::<Account>().values(account(5)).returning::<Account>().execute(&client).await?;
    let plucked =
        insert_many::<Account>().values(vec![account(6), account(7)]).pluck(|x| x.email).execute(&client).await?;

    assert_eq!(returned, account(5));
    assert_eq!(plucked, ["user6@dinoco.rs", "user7@dinoco.rs"]);
    let stored = find_many::<Account>().order_by(|x| x.id.asc()).execute(&client).await?;
    assert_eq!(stored, (3..=7).map(account).collect::<Vec<_>>());
    Ok(())
}

#[tokio::test]
async fn owned_inserts_generate_missing_uuids() -> anyhow::Result<()> {
    let (client, _) = client().await?;
    let mut without_id = account(8);
    without_id.id.clear();
    let mut borrowed_without_id = account(9);
    borrowed_without_id.id.clear();

    insert_into::<Account>().values(without_id).execute(&client).await?;
    insert_into::<Account>().values(&borrowed_without_id).execute(&client).await?;
    let ids = insert_many::<Account>()
        .values(vec![Account { id: String::new(), ..account(10) }, account(11)])
        .pluck(|x| x.id)
        .execute(&client)
        .await?;

    assert_eq!(ids[1], "account-11");
    let stored = find_many::<Account>().pluck(|x| x.id).execute(&client).await?;
    assert_eq!(stored.len(), 4);
    for id in stored.iter().filter(|id| id.as_str() != "account-11") {
        assert_eq!(id.len(), 36, "generated `{id}` is not a UUID");
    }
    Ok(())
}

#[tokio::test]
async fn enum_columns_round_trip_and_filter() -> anyhow::Result<()> {
    let (client, adapter) = client().await?;
    insert_many::<Account>().values((1..=4).map(account)).execute(&client).await?;

    let pro = find_many::<Account>().where_(|x| x.plan.eq(Plan::ProAnnual)).pluck(|x| x.id).execute(&client).await?;
    assert_eq!(pro, ["account-2", "account-4"]);

    adapter.execute("UPDATE rt_account SET plan = 'unknown' WHERE id = 'account-1'", &[]).await?;
    assert!(find_first::<Account>().where_(|x| x.id.eq("account-1")).execute(&client).await.is_err());
    Ok(())
}

#[tokio::test]
async fn many_to_many_disconnect_runs_one_delete_for_every_matched_parent() -> anyhow::Result<()> {
    let (client, _) = client().await?;
    insert_many::<User>()
        .values(vec![
            User::new("user-1".to_string(), "Ada".to_string()),
            User::new("user-2".to_string(), "Grace".to_string()),
            User::new("user-3".to_string(), "Linus".to_string()),
        ])
        .execute(&client)
        .await?;
    insert_many::<Tag>()
        .values((1..=3).map(|index| Tag::new(format!("tag-{index}"), format!("tag {index}"))))
        .execute(&client)
        .await?;
    update_many::<User>()
        .where_(|x| x.id.batch(["user-1", "user-2", "user-3"]))
        .update(|x| x.tag_id.connect_batch(["tag-1", "tag-2", "tag-3"]))
        .execute(&client)
        .await?;

    let deletes = Arc::new(Mutex::new(Vec::new()));
    let recorded = deletes.clone();
    setup_test_methods::<UserTag>(&client).on_delete(move |affected, query, _| {
        recorded.lock().unwrap().push((query.sql.clone(), affected));
    });

    update_many::<User>()
        .where_(|x| x.id.batch(["user-1", "user-2"]))
        .update(|x| x.tag_id.disconnect_batch(["tag-1", "tag-2"]))
        .execute(&client)
        .await?;

    let deletes = deletes.lock().unwrap().clone();
    assert_eq!(deletes.len(), 1, "{deletes:?}");
    assert_eq!(deletes[0].1, Some(4));

    let users = find_many::<User>().order_by(|x| x.id.asc()).includes(|x| x.tags()).execute(&client).await?;
    let tags =
        users.iter().map(|user| user.tags.iter().map(|tag| tag.id.as_str()).collect::<Vec<_>>()).collect::<Vec<_>>();
    assert_eq!(tags, [vec!["tag-3"], vec!["tag-3"], vec!["tag-1", "tag-2", "tag-3"]]);
    Ok(())
}

#[tokio::test]
async fn includes_assign_each_parent_its_own_rows() -> anyhow::Result<()> {
    let (client, _) = client().await?;
    insert_many::<User>()
        .values((1..=4).map(|index| User::new(format!("user-{index}"), format!("user {index}"))))
        .execute(&client)
        .await?;
    insert_many::<Tag>()
        .values((1..=3).map(|index| Tag::new(format!("tag-{index}"), format!("tag {index}"))))
        .execute(&client)
        .await?;
    update_many::<User>()
        .where_(|x| x.id.eq("user-2"))
        .update(|x| x.tag_id.connect_batch(["tag-1", "tag-3"]))
        .execute(&client)
        .await?;
    update_many::<User>().where_(|x| x.id.eq("user-4")).update(|x| x.tag_id.connect("tag-2")).execute(&client).await?;

    let users = find_many::<User>()
        .order_by(|x| x.id.asc())
        .includes(|x| x.tags().order_by(|tag| tag.id.desc()))
        .execute(&client)
        .await?;
    let tags =
        users.iter().map(|user| user.tags.iter().map(|tag| tag.id.as_str()).collect::<Vec<_>>()).collect::<Vec<_>>();
    assert_eq!(tags, [vec![], vec!["tag-3", "tag-1"], vec![], vec!["tag-2"]]);
    Ok(())
}
