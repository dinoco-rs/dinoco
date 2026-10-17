//! Tables and columns named after SQL keywords (`user`, `order`, `group`,
//! `key`, `desc`, `values`, `to`, `rank`) through every builder. A keyword
//! that any of the three databases reserves is quoted in all of them.
//!
//! Every scenario runs on SQLite, PostgreSQL and MySQL.

use std::future::Future;

use dinoco::{
    Entity, EntityExtend, count, delete, delete_many, exists, find_and_update, find_first, find_many, insert_into,
    insert_many, transaction, update, update_many,
};
use dinoco_engine::{
    Backend, DinocoAdapter, DinocoClient, DinocoSqlCompiler, DinocoValue, InsertQuery, MigrationColumnType,
    MySqlAdapter, PostgresAdapter, SqliteAdapter,
};
use dinoco_tests::{column, create_table, drop_table, nullable, primary};

const POSTGRES_URL: &str = "postgres://postgres:postgres@localhost:5432/postgres";
const MYSQL_URL: &str = "mysql://root:root@localhost:3306/mysql";
static POSTGRES_TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
static MYSQL_TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

const TABLES: [&str; 4] = ["user", "order", "group", "_group_to_user"];

macro_rules! on_every_backend {
    ($($scenario:ident),* $(,)?) => {
        $(
            mod $scenario {
                #[tokio::test]
                async fn sqlite() -> anyhow::Result<()> {
                    super::run_on_sqlite(stringify!($scenario), super::$scenario).await
                }

                #[tokio::test]
                async fn postgres() -> anyhow::Result<()> {
                    super::run_on_postgres(super::$scenario).await
                }

                #[tokio::test]
                async fn mysql() -> anyhow::Result<()> {
                    super::run_on_mysql(super::$scenario).await
                }
            }
        )*
    };
}

on_every_backend!(reads, includes_and_relation_filters, writes, transactions);

#[derive(Debug, Entity)]
#[dinoco(table_name = "user")]
pub struct User {
    #[dinoco(primary_key)]
    id: String,
    key: String,
    desc: Option<String>,
    rank: i64,
    to: Option<String>,

    #[dinoco(many_to_one, relation_name = "referral", foreign_key = "to", references = "id")]
    referrer: Option<Box<User>>,

    #[dinoco(one_to_many, relation_name = "referral", foreign_key = "to", references = "id")]
    referred: Vec<User>,

    #[dinoco(one_to_many, foreign_key = "user_id", references = "id")]
    orders: Vec<Order>,

    #[dinoco(
        many_to_many,
        foreign_key = "id",
        references = "id",
        join_table = "_group_to_user",
        parent_field = "id",
        join_parent_field = "user_id",
        join_child_field = "group_id"
    )]
    groups: Vec<Group>,
}

#[derive(Debug, EntityExtend)]
#[extend(User)]
pub struct UserKey {
    pub key: String,
    pub desc: Option<String>,
}

#[derive(Debug, Entity)]
#[dinoco(table_name = "order")]
pub struct Order {
    #[dinoco(primary_key)]
    id: String,
    values: String,
    to: Option<String>,
    user_id: Option<String>,

    #[dinoco(many_to_one, foreign_key = "user_id", references = "id")]
    user: Option<User>,
}

#[derive(Debug, Entity)]
#[dinoco(table_name = "group")]
pub struct Group {
    #[dinoco(primary_key)]
    id: String,
    order: i64,

    #[dinoco(
        many_to_many,
        foreign_key = "id",
        references = "id",
        join_table = "_group_to_user",
        parent_field = "id",
        join_parent_field = "group_id",
        join_child_field = "user_id"
    )]
    users: Vec<User>,
}

async fn reads(client: DinocoClient) -> anyhow::Result<()> {
    let users =
        find_many::<User>().where_(|user| user.rank.gte(2)).order_by(|user| user.key.desc()).execute(&client).await?;
    assert_eq!(ids(&users, |user| &user.id), ["u3", "u2"]);

    let first = find_first::<User>().where_(|user| user.desc.null()).execute(&client).await?.expect("u2");
    assert_eq!(first.id, "u2");

    let keys = find_many::<User>()
        .select::<UserKey>()
        .where_complex(|user, m| m.or(user.key.eq("k1"), user.desc.eq("third")))
        .order_by(|user| user.rank.asc())
        .execute(&client)
        .await?;
    assert_eq!(keys.iter().map(|user| user.key.as_str()).collect::<Vec<_>>(), ["k1", "k3"]);
    assert_eq!(keys[1].desc.as_deref(), Some("third"));

    let groups = find_many::<Group>().order_by(|group| group.order.desc()).execute(&client).await?;
    assert_eq!(ids(&groups, |group| &group.id), ["g2", "g1"]);

    assert_eq!(count::<Order>().where_(|order| order.to.not_null()).execute(&client).await?.total, 1);
    assert!(exists::<Order>().where_(|order| order.values.eq("b")).execute(&client).await?);
    assert!(!exists::<Order>().where_(|order| order.values.eq("missing")).execute(&client).await?);
    Ok(())
}

async fn includes_and_relation_filters(client: DinocoClient) -> anyhow::Result<()> {
    let users = find_many::<User>()
        .includes(|user| user.orders().order_by(|order| order.values.asc()))
        .includes(|user| user.groups().order_by(|group| group.order.asc()))
        .order_by(|user| user.id.asc())
        .execute(&client)
        .await?;
    let loaded = users
        .iter()
        .map(|user| (user.id.as_str(), ids(&user.orders, |order| &order.id), ids(&user.groups, |group| &group.id)))
        .collect::<Vec<_>>();
    assert_eq!(
        loaded,
        [
            ("u1", vec!["o1".to_string(), "o2".to_string()], vec!["g1".to_string(), "g2".to_string()]),
            ("u2", vec!["o3".to_string()], vec!["g2".to_string()]),
            ("u3", vec![], vec![]),
        ]
    );

    // The partitioned include query of `take`.
    let users = find_many::<User>()
        .includes(|user| user.orders().order_by(|order| order.values.desc()).take(1))
        .order_by(|user| user.id.asc())
        .execute(&client)
        .await?;
    assert_eq!(ids(&users[0].orders, |order| &order.id), ["o2"]);

    let orders = find_many::<Order>()
        .includes(|order| order.user())
        .where_(|order| order.user.where_(|user| user.key.eq("k1")))
        .order_by(|order| order.id.asc())
        .execute(&client)
        .await?;
    assert_eq!(ids(&orders, |order| &order.id), ["o1", "o2"]);
    assert!(orders.iter().all(|order| order.user.as_ref().is_some_and(|user| user.key == "k1")));

    let groups = find_many::<Group>()
        .where_(|group| group.users.every(|user| user.rank.lt(3)))
        .where_(|group| group.users.exists())
        .order_by(|group| group.id.asc())
        .execute(&client)
        .await?;
    assert_eq!(ids(&groups, |group| &group.id), ["g1", "g2"]);
    let lonely =
        find_many::<User>().where_(|user| user.groups.none(|group| group.order.eq(2))).execute(&client).await?;
    assert_eq!(ids(&lonely, |user| &user.id), ["u3"]);

    let counted = count::<User>()
        .where_(|user| user.id.eq("u1"))
        .includes(|user| user.orders().where_(|order| order.to.null()))
        .includes(|user| user.groups())
        .execute(&client)
        .await?;
    assert_eq!((counted.orders, counted.groups), (Some(1), Some(2)));

    let groups = find_many::<Group>()
        .includes(|group| group.users().order_by(|user| user.rank.desc()).take(1))
        .order_by(|group| group.id.asc())
        .execute(&client)
        .await?;
    let loaded = groups.iter().map(|group| ids(&group.users, |user| &user.id)).collect::<Vec<_>>();
    assert_eq!(loaded, [vec!["u1".to_string()], vec!["u2".to_string()]]);

    // A self relation joins the table to itself under aliases.
    let referrer = find_first::<User>()
        .where_(|user| user.id.eq("u1"))
        .includes(|user| user.referred().order_by(|user| user.id.asc()))
        .execute(&client)
        .await?
        .expect("u1");
    assert_eq!(ids(&referrer.referred, |user| &user.id), ["u2", "u3"]);
    let referred = find_many::<User>()
        .where_(|user| user.referrer.where_(|referrer| referrer.key.eq("k1")))
        .includes(|user| user.referrer())
        .order_by(|user| user.id.asc())
        .execute(&client)
        .await?;
    assert_eq!(ids(&referred, |user| &user.id), ["u2", "u3"]);
    assert!(referred.iter().all(|user| user.referrer.as_ref().is_some_and(|referrer| referrer.id == "u1")));
    Ok(())
}

async fn writes(client: DinocoClient) -> anyhow::Result<()> {
    let mut user = User::new("u4".to_string(), "k4".to_string(), 9);
    user.desc = Some("fourth".to_string());
    insert_into::<User>().values(&user).execute(&client).await?;

    update::<User>().where_(|user| user.key.eq("k4")).update(|user| user.rank.increment(1)).execute(&client).await?;
    let updated = update_many::<Order>()
        .where_(|order| order.user.where_(|user| user.key.eq("k1")))
        .update(|order| order.to.set(Some("shipped".to_string())))
        .returning::<Order>()
        .execute(&client)
        .await?;
    let mut updated = ids(&updated, |order| &order.id);
    updated.sort();
    assert_eq!(updated, ["o1", "o2"]);

    let bumped = find_and_update::<User>()
        .where_(|user| user.desc.eq("fourth"))
        .update(|user| user.desc.set(Some("renamed".to_string())))
        .execute(&client)
        .await?;
    assert_eq!((bumped.id.as_str(), bumped.rank, bumped.desc.as_deref()), ("u4", 10, Some("renamed")));

    delete::<Order>().where_(|order| order.values.eq("c")).execute(&client).await?;
    delete_many::<User>().where_(|user| user.orders.not_exists()).execute(&client).await?;
    let remaining = find_many::<User>().order_by(|user| user.id.asc()).execute(&client).await?;
    assert_eq!(ids(&remaining, |user| &user.id), ["u1"]);
    Ok(())
}

async fn transactions(client: DinocoClient) -> anyhow::Result<()> {
    let orders = transaction(&client, |tx| async move {
        insert_into::<Order>().values(&Order::new("o4".to_string(), "d".to_string())).execute(tx).await?;
        update_many::<Order>()
            .where_(|order| order.user_id.null())
            .update(|order| order.to.set(Some("pending".to_string())))
            .execute(tx)
            .await?;
        find_many::<Order>().where_(|order| order.to.eq("pending")).execute(tx).await
    })
    .await?;
    assert_eq!(ids(&orders, |order| &order.id), ["o4"]);

    let rolled_back: Result<(), _> = transaction(&client, |tx| async move {
        delete_many::<Order>().where_(|order| order.values.not_null()).execute(tx).await?;
        anyhow::ensure!(!exists::<Order>().execute(tx).await?, "the delete is visible inside the transaction");
        anyhow::bail!("roll back")
    })
    .await;
    assert!(rolled_back.is_err());
    assert_eq!(count::<Order>().execute(&client).await?.total, 4, "the rolled back delete kept every order");
    Ok(())
}

fn ids<T>(rows: &[T], id: impl Fn(&T) -> &String) -> Vec<String> {
    rows.iter().map(|row| id(row).clone()).collect()
}

async fn run_on_sqlite<F, Fut>(name: &str, scenario: F) -> anyhow::Result<()>
where
    F: FnOnce(DinocoClient) -> Fut,
    Fut: Future<Output = anyhow::Result<()>>,
{
    let path =
        std::env::temp_dir().join(format!("dinoco-reserved-{name}-{}-{}.sqlite", std::process::id(), monotonic()));
    let adapter = SqliteAdapter::new(path.to_string_lossy().into_owned()).await.map_err(anyhow::Error::msg)?;
    reset_schema(&adapter).await?;
    let result = run_seeded(DinocoClient::new(Backend::Sqlite(adapter)), scenario).await;
    let _ = std::fs::remove_file(path);
    result
}

async fn run_on_postgres<F, Fut>(scenario: F) -> anyhow::Result<()>
where
    F: FnOnce(DinocoClient) -> Fut,
    Fut: Future<Output = anyhow::Result<()>>,
{
    let _guard = POSTGRES_TEST_LOCK.lock().await;
    let adapter = PostgresAdapter::direct(POSTGRES_URL).await?;
    reset_schema(&adapter).await?;
    run_seeded(DinocoClient::new(Backend::Postgres(adapter)), scenario).await
}

async fn run_on_mysql<F, Fut>(scenario: F) -> anyhow::Result<()>
where
    F: FnOnce(DinocoClient) -> Fut,
    Fut: Future<Output = anyhow::Result<()>>,
{
    let _guard = MYSQL_TEST_LOCK.lock().await;
    let adapter = MySqlAdapter::new(MYSQL_URL);
    reset_schema(&adapter).await?;
    run_seeded(DinocoClient::new(Backend::Mysql(adapter)), scenario).await
}

async fn run_seeded<F, Fut>(client: DinocoClient, scenario: F) -> anyhow::Result<()>
where
    F: FnOnce(DinocoClient) -> Fut,
    Fut: Future<Output = anyhow::Result<()>>,
{
    seed(&client).await?;
    scenario(client).await
}

async fn reset_schema<A>(adapter: &A) -> anyhow::Result<()>
where
    A: DinocoAdapter + DinocoSqlCompiler,
{
    for table in TABLES.iter().rev() {
        drop_table(adapter, table).await?;
    }

    let string = |name: &str| column(name, MigrationColumnType::String);
    let integer = |name: &str| column(name, MigrationColumnType::Integer);
    create_table(
        adapter,
        "user",
        vec![primary(string("id")), string("key"), nullable(string("desc")), integer("rank"), nullable(string("to"))],
    )
    .await?;
    create_table(
        adapter,
        "order",
        vec![primary(string("id")), string("values"), nullable(string("to")), nullable(string("user_id"))],
    )
    .await?;
    create_table(adapter, "group", vec![primary(string("id")), integer("order")]).await?;
    create_table(adapter, "_group_to_user", vec![string("group_id"), string("user_id")]).await
}

/// Seeds:
///
/// u1 referred u2 and u3 (`user.to`).
///
/// | user | key | desc  | rank | orders (values, to)     | groups |
/// | ---- | --- | ----- | ---- | ----------------------- | ------ |
/// | u1   | k1  | first | 1    | o1 (a, -), o2 (b, gift) | g1, g2 |
/// | u2   | k2  | NULL  | 2    | o3 (c, -)               | g2     |
/// | u3   | k3  | third | 3    |                         |        |
///
/// Groups: g1 (order 1), g2 (order 2).
async fn seed(client: &DinocoClient) -> anyhow::Result<()> {
    let user = |id: &str, key: &str, desc: Option<&str>, rank| {
        let mut user = User::new(id.to_string(), key.to_string(), rank);
        user.desc = desc.map(str::to_string);
        // u1 referred u2 and u3.
        user.to = (id != "u1").then(|| "u1".to_string());
        user
    };
    insert_many::<User>()
        .values(vec![user("u1", "k1", Some("first"), 1), user("u2", "k2", None, 2), user("u3", "k3", Some("third"), 3)])
        .execute(client)
        .await?;

    let order = |id: &str, values: &str, to: Option<&str>, user_id: &str| {
        let mut order = Order::new(id.to_string(), values.to_string());
        order.to = to.map(str::to_string);
        order.user_id = Some(user_id.to_string());
        order
    };
    insert_many::<Order>()
        .values(vec![order("o1", "a", None, "u1"), order("o2", "b", Some("gift"), "u1"), order("o3", "c", None, "u2")])
        .execute(client)
        .await?;

    insert_many::<Group>()
        .values(vec![Group::new("g1".to_string(), 1), Group::new("g2".to_string(), 2)])
        .execute(client)
        .await?;
    let link = |group_id: &str, user_id: &str| {
        vec![DinocoValue::String(group_id.to_string()), DinocoValue::String(user_id.to_string())]
    };
    client
        .backend
        .insert(InsertQuery {
            table: "_group_to_user",
            fields: vec!["group_id", "user_id"],
            rows: vec![link("g1", "u1"), link("g2", "u1"), link("g2", "u2")],
            returning: None,
        })
        .await?;
    Ok(())
}

fn monotonic() -> u128 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
}
