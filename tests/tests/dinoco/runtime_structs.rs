//! `#[derive(DinocoRuntimeStruct)]`: structs kept in a column of another
//! type. Covers a newtype read through `From` (`Amount(i64)`), a newtype read
//! through a validating `TryFrom` (`Sku(String)`), a named struct with
//! `#[dinoco(column = i64)]` (`Cents`) and a serde struct in a `Json` column
//! (`Metadata`), optional or not: inserts, reads, the schema default,
//! filters, `where_complex`, updates, `find_and_update`, `sum`/`avg`/`min`/
//! `max`/`count`, `pluck`, `group_by`, includes and relation filters, stored
//! values the struct rejects, transactions and the generated conversions.
//!
//! Every scenario runs on SQLite, PostgreSQL and MySQL (the same local servers
//! `docker_adapters.rs` uses).

use std::future::Future;

use dinoco::serde::{Deserialize, Serialize};
use dinoco::{
    DinocoRuntimeStruct, DinocoValue, Entity, IntoUpdateValue, find_and_update, find_first, find_many, insert_into,
    insert_many, transaction, update, update_many,
};
use dinoco_engine::{
    Backend, DinocoAdapter, DinocoClient, DinocoSqlCompiler, MigrationColumnType, MySqlAdapter, PostgresAdapter,
    SqliteAdapter,
};
use dinoco_tests::{column, create_table, drop_table, nullable, primary};

const POSTGRES_URL: &str = "postgres://dinoco:dinoco@localhost:5432/dinoco";
const MYSQL_URL: &str = "mysql://dinoco:dinoco@localhost:3306/dinoco";
static POSTGRES_TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
static MYSQL_TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

const TABLES: [&str; 2] = ["rs_customer", "rs_order"];

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

on_every_backend!(
    writes_columns_and_reads_structs,
    filters,
    updates,
    aggregates,
    relations,
    rejected_stored_values,
    transactions,
);

/// A newtype read back through `From`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, DinocoRuntimeStruct)]
#[serde(crate = "dinoco::serde")]
pub struct Amount(i64);

impl From<i64> for Amount {
    fn from(cents: i64) -> Self {
        Amount(cents)
    }
}

/// A newtype that validates what it reads.
#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, DinocoRuntimeStruct)]
#[serde(crate = "dinoco::serde")]
pub struct Sku(String);

impl TryFrom<String> for Sku {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        if value.starts_with("SKU-") { Ok(Sku(value)) } else { Err(format!("`{value}` is not a SKU")) }
    }
}

/// A named struct over an integer column.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, DinocoRuntimeStruct)]
#[serde(crate = "dinoco::serde")]
#[dinoco(column = i64)]
pub struct Cents {
    value: u32,
}

impl From<&Cents> for i64 {
    fn from(cents: &Cents) -> Self {
        i64::from(cents.value)
    }
}

impl TryFrom<i64> for Cents {
    type Error = std::num::TryFromIntError;

    fn try_from(value: i64) -> Result<Self, Self::Error> {
        Ok(Cents { value: u32::try_from(value)? })
    }
}

/// A serde struct in a `Json` column.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, DinocoRuntimeStruct)]
#[serde(crate = "dinoco::serde")]
#[dinoco(json)]
pub struct Metadata {
    source: String,
    tags: Vec<String>,
    retries: u32,
}

#[derive(Debug, Clone, Entity)]
#[dinoco(table_name = "rs_customer")]
pub struct Customer {
    #[dinoco(primary_key)]
    id: String,
    name: String,

    #[dinoco(one_to_many, foreign_key = "customer_id", references = "id")]
    orders: Vec<Order>,
}

/// What `field.set_runtime_struct(...)` generates for `amount Integer
/// @default(0)`, `discount Integer?`, `sku String`, `fee Integer`, `metadata
/// Json` and `extra Json?`.
#[derive(Debug, Clone, Entity)]
#[dinoco(table_name = "rs_order")]
pub struct Order {
    #[dinoco(primary_key)]
    id: String,
    customer_id: String,
    #[dinoco(
        default = <crate::Amount as ::dinoco::DinocoRuntimeStruct>::from_column(::core::convert::Into::into(0)).expect("the @default of a runtime struct field must be a value of `crate::Amount`")
    )]
    #[dinoco(scalar)]
    amount: crate::Amount,
    #[dinoco(scalar)]
    discount: Option<crate::Amount>,
    #[dinoco(scalar)]
    sku: crate::Sku,
    #[dinoco(scalar)]
    fee: crate::Cents,
    #[dinoco(scalar)]
    metadata: crate::Metadata,
    #[dinoco(scalar)]
    extra: Option<crate::Metadata>,

    #[dinoco(many_to_one, foreign_key = "customer_id", references = "id")]
    customer: Option<Box<Customer>>,
}

/// The same table, read through the schema's plain types.
#[derive(Debug, Clone, Entity)]
#[dinoco(table_name = "rs_order")]
pub struct RawOrder {
    #[dinoco(primary_key)]
    id: String,
    customer_id: String,
    amount: i64,
    discount: Option<i64>,
    sku: String,
    fee: i64,
    metadata: dinoco::serde_json::Value,
    extra: Option<dinoco::serde_json::Value>,
}

fn metadata(source: &str, tags: &[&str]) -> Metadata {
    Metadata { source: source.to_string(), tags: tags.iter().map(|tag| tag.to_string()).collect(), retries: 0 }
}

fn order(id: &str, customer: &str, amount: i64, discount: Option<i64>, sku: &str) -> Order {
    Order {
        id: id.to_string(),
        customer_id: customer.to_string(),
        amount: Amount(amount),
        discount: discount.map(Amount),
        sku: Sku(sku.to_string()),
        fee: Cents { value: 25 },
        metadata: metadata("web", &["new"]),
        extra: None,
        customer: None,
    }
}

async fn seed(client: &DinocoClient) -> anyhow::Result<()> {
    insert_many::<Customer>()
        .values(["c1", "c2", "c3"].map(|id| Customer::new(id.to_string(), format!("{id} name"))))
        .execute(client)
        .await?;
    let mut o3 = order("o3", "c2", 300, None, "SKU-A");
    o3.extra = Some(metadata("app", &["vip", "gift"]));
    insert_many::<Order>()
        .values([
            order("o1", "c1", 100, Some(10), "SKU-A"),
            order("o2", "c1", 250, None, "SKU-B"),
            o3,
            order("o4", "c2", 50, Some(5), "SKU-B"),
        ])
        .execute(client)
        .await?;
    Ok(())
}

async fn typed(client: &DinocoClient, id: &str) -> anyhow::Result<Order> {
    let id = id.to_string();
    Ok(find_first::<Order>().where_(|x| x.id.eq(id)).execute(client).await?.expect("order"))
}

async fn raw(client: &DinocoClient, id: &str) -> anyhow::Result<RawOrder> {
    let id = id.to_string();
    Ok(find_first::<RawOrder>().where_(|x| x.id.eq(id)).execute(client).await?.expect("raw order"))
}

async fn ids<F>(client: &DinocoClient, filter: F) -> anyhow::Result<Vec<String>>
where
    F: FnOnce(OrderWhere) -> dinoco::FindWhere,
{
    find_many::<Order>().where_(filter).order_by(|x| x.id.asc()).pluck(|x| x.id).execute(client).await
}

async fn writes_columns_and_reads_structs(client: DinocoClient) -> anyhow::Result<()> {
    // Each column holds the plain value.
    let stored = raw(&client, "o3").await?;
    assert_eq!((stored.amount, stored.discount, stored.sku.as_str(), stored.fee), (300, None, "SKU-A", 25));
    assert_eq!(stored.metadata, dinoco::serde_json::json!({ "source": "web", "tags": ["new"], "retries": 0 }));
    assert_eq!(
        stored.extra,
        Some(dinoco::serde_json::json!({ "source": "app", "tags": ["vip", "gift"], "retries": 0 }))
    );
    assert_eq!(raw(&client, "o1").await?.discount, Some(10));

    // And reads back as the structs.
    let read = typed(&client, "o3").await?;
    assert_eq!(
        (read.amount, read.discount, read.sku.clone(), read.fee),
        (Amount(300), None, Sku("SKU-A".into()), Cents { value: 25 })
    );
    assert_eq!(read.metadata, metadata("web", &["new"]));
    assert_eq!(read.extra, Some(metadata("app", &["vip", "gift"])));
    assert_eq!(typed(&client, "o1").await?.discount, Some(Amount(10)));

    // The schema default goes through the struct.
    let defaulted = Order::new(
        "o5".to_string(),
        "c3".to_string(),
        Sku("SKU-C".to_string()),
        Cents { value: 0 },
        Metadata::default(),
    );
    assert_eq!(defaulted.amount, Amount(0));
    insert_into::<Order>().values(&defaulted).execute(&client).await?;
    assert_eq!(raw(&client, "o5").await?.amount, 0);

    // `returning` reads the structs back.
    let returned = insert_into::<Order>()
        .values(order("o6", "c3", 75, Some(1), "SKU-D"))
        .returning::<Order>()
        .execute(&client)
        .await?;
    assert_eq!((returned.amount, returned.discount), (Amount(75), Some(Amount(1))));
    Ok(())
}

async fn filters(client: DinocoClient) -> anyhow::Result<()> {
    assert_eq!(ids(&client, |x| x.amount.eq(Amount(250))).await?, ["o2"]);
    assert_eq!(ids(&client, |x| x.amount.gte(Amount(250))).await?, ["o2", "o3"]);
    // The column's own type works as a value too.
    assert_eq!(ids(&client, |x| x.amount.lt(100_i64)).await?, ["o4"]);
    assert_eq!(ids(&client, |x| x.amount.batch([Amount(50), Amount(300)])).await?, ["o3", "o4"]);
    assert_eq!(ids(&client, |x| x.amount.not_in_batch([Amount(50), Amount(300)])).await?, ["o1", "o2"]);
    assert_eq!(ids(&client, |x| x.sku.eq(&Sku("SKU-B".to_string()))).await?, ["o2", "o4"]);
    assert_eq!(ids(&client, |x| x.fee.eq(Cents { value: 25 })).await?.len(), 4);
    assert_eq!(ids(&client, |x| x.discount.null()).await?, ["o2", "o3"]);
    assert_eq!(ids(&client, |x| x.discount.gt(Amount(6))).await?, ["o1"]);
    assert_eq!(ids(&client, |x| x.extra.not_null()).await?, ["o3"]);

    let rows = find_many::<Order>()
        .where_complex(|x, c| c.or(x.amount.gt(Amount(200)), x.discount.eq(Amount(5))))
        .order_by(|x| x.amount.desc())
        .pluck(|x| x.id)
        .execute(&client)
        .await?;
    assert_eq!(rows, ["o3", "o2", "o4"]);
    Ok(())
}

async fn updates(client: DinocoClient) -> anyhow::Result<()> {
    update::<Order>()
        .where_(|x| x.id.eq("o1"))
        .update(|x| x.amount.set(Amount(111)))
        .update(|x| x.discount.set(Amount(11)))
        .update(|x| x.sku.set(Sku("SKU-Z".to_string())))
        .update(|x| x.fee.set(&Cents { value: 7 }))
        .update(|x| x.metadata.set(metadata("api", &["edited"])))
        .update(|x| x.extra.set(metadata("api", &[])))
        .execute(&client)
        .await?;
    let read = typed(&client, "o1").await?;
    assert_eq!((read.amount, read.discount, read.fee), (Amount(111), Some(Amount(11)), Cents { value: 7 }));
    assert_eq!(read.sku, Sku("SKU-Z".to_string()));
    assert_eq!((read.metadata.source.as_str(), read.extra.map(|extra| extra.source)), ("api", Some("api".to_string())));

    update_many::<Order>()
        .where_(|x| x.customer_id.eq("c1"))
        .update(|x| x.discount.set(Amount(0)))
        .execute(&client)
        .await?;
    assert_eq!(raw(&client, "o2").await?.discount, Some(0));

    let updated = find_and_update::<Order>()
        .where_(|x| x.id.eq("o4"))
        .update(|x| x.amount.set(Amount(55)))
        .execute(&client)
        .await?;
    assert_eq!(updated.amount, Amount(55));
    Ok(())
}

async fn aggregates(client: DinocoClient) -> anyhow::Result<()> {
    // A sum of amounts is an amount.
    let total: Amount = find_many::<Order>().pluck(|x| x.amount.sum()).execute(&client).await?;
    assert_eq!(total, Amount(700));
    let discounts: Amount = find_many::<Order>().pluck(|x| x.discount.sum()).execute(&client).await?;
    assert_eq!(discounts, Amount(15));
    let average = find_many::<Order>().pluck(|x| x.amount.avg()).execute(&client).await?;
    assert_eq!(average, Some(175.0));
    let smallest: Option<Amount> = find_many::<Order>().pluck(|x| x.amount.min()).execute(&client).await?;
    assert_eq!(smallest, Some(Amount(50)));
    let largest: Option<Sku> = find_many::<Order>().pluck(|x| x.sku.max()).execute(&client).await?;
    assert_eq!(largest, Some(Sku("SKU-B".to_string())));
    let none: Option<Amount> =
        find_many::<Order>().where_(|x| x.id.eq("none")).pluck(|x| x.amount.max()).execute(&client).await?;
    assert_eq!(none, None);
    assert_eq!(find_many::<Order>().pluck(|x| x.discount.count()).execute(&client).await?, 2);

    let amounts = find_many::<Order>().order_by(|x| x.amount.asc()).pluck(|x| x.amount).execute(&client).await?;
    assert_eq!(amounts, [Amount(50), Amount(100), Amount(250), Amount(300)]);
    let extras = find_many::<Order>().order_by(|x| x.id.asc()).pluck(|x| x.extra).execute(&client).await?;
    assert_eq!(extras, [None, None, Some(metadata("app", &["vip", "gift"])), None]);

    // Grouped by a runtime struct, summing another.
    let by_sku = find_many::<Order>().pluck(|x| x.amount.sum()).group_by(|x| x.sku).execute(&client).await?;
    assert_eq!(by_sku, [(Sku("SKU-A".to_string()), Amount(400)), (Sku("SKU-B".to_string()), Amount(300))]);
    Ok(())
}

async fn relations(client: DinocoClient) -> anyhow::Result<()> {
    let customers = find_many::<Customer>()
        .where_(|x| x.orders.where_(|o| o.amount.gte(Amount(250))))
        .includes(|x| x.orders().where_(|o| o.discount.not_null()).order_by(|o| o.amount.asc()))
        .order_by(|x| x.id.asc())
        .execute(&client)
        .await?;
    let shape = customers
        .iter()
        .map(|customer| (customer.id.as_str(), customer.orders.iter().map(|order| order.amount).collect::<Vec<_>>()))
        .collect::<Vec<_>>();
    assert_eq!(shape, [("c1", vec![Amount(100)]), ("c2", vec![Amount(50)])]);

    let order = find_first::<Order>()
        .where_(|x| x.id.eq("o3"))
        .includes(|x| x.customer())
        .execute(&client)
        .await?
        .expect("order");
    assert_eq!(order.customer.map(|customer| customer.id), Some("c2".to_string()));
    Ok(())
}

async fn rejected_stored_values(client: DinocoClient) -> anyhow::Result<()> {
    // Values the structs reject, written through the plain model.
    let mut bad = RawOrder {
        id: "bad-sku".to_string(),
        customer_id: "c3".to_string(),
        amount: 1,
        discount: None,
        sku: "not-a-sku".to_string(),
        fee: 1,
        metadata: dinoco::serde_json::json!({ "source": "web", "tags": [], "retries": 0 }),
        extra: None,
    };
    insert_into::<RawOrder>().values(&bad).execute(&client).await?;
    assert!(find_first::<Order>().where_(|x| x.id.eq("bad-sku")).execute(&client).await.is_err());

    bad.id = "bad-fee".to_string();
    bad.sku = "SKU-OK".to_string();
    bad.fee = -1;
    insert_into::<RawOrder>().values(&bad).execute(&client).await?;
    assert!(find_first::<Order>().where_(|x| x.id.eq("bad-fee")).execute(&client).await.is_err());

    bad.id = "bad-json".to_string();
    bad.fee = 1;
    bad.metadata = dinoco::serde_json::json!({ "source": 42 });
    insert_into::<RawOrder>().values(&bad).execute(&client).await?;
    assert!(find_first::<Order>().where_(|x| x.id.eq("bad-json")).execute(&client).await.is_err());

    // The rows are still readable as their plain types.
    assert_eq!(find_many::<RawOrder>().execute(&client).await?.len(), 7);
    Ok(())
}

async fn transactions(client: DinocoClient) -> anyhow::Result<()> {
    let total = transaction(&client, |tx| async move {
        insert_into::<Order>().values(order("o9", "c3", 1_000, None, "SKU-C")).execute(tx).await?;
        update::<Order>().where_(|x| x.id.eq("o1")).update(|x| x.amount.set(Amount(0))).execute(tx).await?;
        Ok(find_many::<Order>().pluck(|x| x.amount.sum()).execute(tx).await?)
    })
    .await?;
    assert_eq!(total, Amount(1_600));
    Ok(())
}

#[test]
fn generated_conversions() {
    assert_eq!(DinocoValue::from(Amount(5)), DinocoValue::Integer(5));
    assert_eq!(DinocoValue::from(&Sku("SKU-A".to_string())), DinocoValue::String("SKU-A".to_string()));
    assert_eq!(DinocoValue::from(Cents { value: 9 }), DinocoValue::Integer(9));
    assert_eq!(
        DinocoValue::from(&metadata("web", &["a"])),
        DinocoValue::Json(dinoco::serde_json::json!({ "source": "web", "tags": ["a"], "retries": 0 }))
    );

    assert_eq!(Amount(3).to_column(), 3);
    assert_eq!(Amount::from_column(4), Ok(Amount(4)));
    assert_eq!(Sku::from_column("nope".to_string()), Err("`nope` is not a SKU".to_string()));
    assert!(Cents::from_column(-1).is_err());
    let error = Metadata::from_column(dinoco::serde_json::json!([])).expect_err("not an object");
    assert!(error.contains("`Metadata` could not be read from JSON"), "{error}");

    assert_eq!(IntoUpdateValue::<Amount>::into_update_value(Amount(1)), DinocoValue::Integer(1));
    assert_eq!(IntoUpdateValue::<Option<Amount>>::into_update_value(&Amount(2)), DinocoValue::Integer(2));
}

async fn run_on_sqlite<F, Fut>(name: &str, scenario: F) -> anyhow::Result<()>
where
    F: FnOnce(DinocoClient) -> Fut,
    Fut: Future<Output = anyhow::Result<()>>,
{
    let path = format!("/private/tmp/dinoco-rs-{name}-{}-{}.sqlite", std::process::id(), monotonic());
    let adapter = SqliteAdapter::new(path.clone()).await.map_err(anyhow::Error::msg)?;
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
    let json = |name: &str| column(name, MigrationColumnType::Json);
    create_table(adapter, "rs_customer", vec![primary(string("id")), string("name")]).await?;
    create_table(
        adapter,
        "rs_order",
        vec![
            primary(string("id")),
            string("customer_id"),
            integer("amount"),
            nullable(integer("discount")),
            string("sku"),
            integer("fee"),
            json("metadata"),
            nullable(json("extra")),
        ],
    )
    .await
}

fn monotonic() -> u128 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
}
