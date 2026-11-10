//! `in_query(...)`/`not_in_query(...)`: a field filtered by the values another
//! query plucks, `<field> [NOT] IN (SELECT ...)` in the same statement.
//! Covers `find_many` and `find_first` subqueries, optional columns on either
//! side, windows (`order_by` + `take`/`skip`), `NULL` in a `NOT IN`, empty
//! subqueries, nesting, relation filters both ways, `where_complex`, every
//! builder that takes a `where_` (writes on the same table included, which
//! MySQL only accepts through a derived table), cursors, transactions and the
//! SQL each database gets.
//!
//! Every scenario runs on SQLite, PostgreSQL and MySQL (the same local servers
//! `docker_adapters.rs` uses).

use std::future::Future;

use dinoco::{
    DinocoValue, Entity, FindQuery, FindWhere, SubQuery, count, delete_many, exists, find_first, find_many,
    insert_many, transaction, update_many,
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

const TABLES: [&str; 2] = ["iq_customer", "iq_order"];

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
    keeps_rows_in_the_subquery,
    optional_columns_and_nulls,
    windows_and_find_first,
    nested_and_relation_filters,
    where_complex_and_reads,
    writes_on_the_same_table,
    empty_subqueries,
    cursors,
    transactions,
);

#[derive(Debug, Clone, Entity)]
#[dinoco(table_name = "iq_customer")]
pub struct Customer {
    #[dinoco(primary_key)]
    id: String,
    vip: bool,
    region: String,

    #[dinoco(one_to_many, foreign_key = "customer_id", references = "id")]
    orders: Vec<Order>,
}

#[derive(Debug, Clone, Entity)]
#[dinoco(table_name = "iq_order")]
pub struct Order {
    #[dinoco(primary_key)]
    id: String,
    customer_id: Option<String>,
    amount: i64,
    status: String,

    #[dinoco(many_to_one, foreign_key = "customer_id", references = "id")]
    customer: Option<Box<Customer>>,
}

/// Customers: c1 and c3 are VIP; c4 has no orders.
/// Orders: o1 (c1, 100, paid), o2 (c1, 250, pending), o3 (c2, 400, paid),
/// o4 (c3, 50, refunded), o5 (no customer, 75, paid).
async fn seed(client: &DinocoClient) -> anyhow::Result<()> {
    insert_many::<Customer>()
        .values([("c1", true, "south"), ("c2", false, "north"), ("c3", true, "north"), ("c4", false, "south")].map(
            |(id, vip, region)| Customer { id: id.to_string(), vip, region: region.to_string(), orders: Vec::new() },
        ))
        .execute(client)
        .await?;
    insert_many::<Order>()
        .values(
            [
                ("o1", Some("c1"), 100, "paid"),
                ("o2", Some("c1"), 250, "pending"),
                ("o3", Some("c2"), 400, "paid"),
                ("o4", Some("c3"), 50, "refunded"),
                ("o5", None, 75, "paid"),
            ]
            .map(|(id, customer, amount, status)| Order {
                id: id.to_string(),
                customer_id: customer.map(str::to_string),
                amount,
                status: status.to_string(),
                customer: None,
            }),
        )
        .execute(client)
        .await?;
    Ok(())
}

async fn orders<F>(client: &DinocoClient, filter: F) -> anyhow::Result<Vec<String>>
where
    F: FnOnce(OrderWhere) -> FindWhere,
{
    find_many::<Order>().where_(filter).order_by(|x| x.id.asc()).pluck(|x| x.id).execute(client).await
}

async fn customers<F>(client: &DinocoClient, filter: F) -> anyhow::Result<Vec<String>>
where
    F: FnOnce(CustomerWhere) -> FindWhere,
{
    find_many::<Customer>().where_(filter).order_by(|x| x.id.asc()).pluck(|x| x.id).execute(client).await
}

fn vip_ids() -> dinoco::Pluck<Customer, String> {
    find_many::<Customer>().where_(|c| c.vip.eq(true)).pluck(|c| c.id)
}

async fn keeps_rows_in_the_subquery(client: DinocoClient) -> anyhow::Result<()> {
    assert_eq!(orders(&client, |x| x.customer_id.in_query(vip_ids())).await?, ["o1", "o2", "o4"]);
    // A `NULL` customer is in neither the list nor its complement.
    assert_eq!(orders(&client, |x| x.customer_id.not_in_query(vip_ids())).await?, ["o3"]);
    // Combined with other filters on the same builder.
    let rows = find_many::<Order>()
        .where_(|x| x.customer_id.in_query(vip_ids()))
        .where_(|x| x.amount.gte(100_i64))
        .order_by(|x| x.id.asc())
        .pluck(|x| x.id)
        .execute(&client)
        .await?;
    assert_eq!(rows, ["o1", "o2"]);
    Ok(())
}

async fn optional_columns_and_nulls(client: DinocoClient) -> anyhow::Result<()> {
    // A plain column against an optional one.
    let big_spenders = find_many::<Order>().where_(|o| o.amount.gte(100_i64)).pluck(|o| o.customer_id);
    assert_eq!(customers(&client, |x| x.id.in_query(big_spenders)).await?, ["c1", "c2"]);

    // `NOT IN` a list holding `NULL` keeps nothing, as in SQL.
    let every_customer = find_many::<Order>().pluck(|o| o.customer_id);
    assert!(customers(&client, |x| x.id.not_in_query(every_customer)).await?.is_empty());
    let known_customers = find_many::<Order>().where_(|o| o.customer_id.not_null()).pluck(|o| o.customer_id);
    assert_eq!(customers(&client, |x| x.id.not_in_query(known_customers)).await?, ["c4"]);
    Ok(())
}

async fn windows_and_find_first(client: DinocoClient) -> anyhow::Result<()> {
    // The customer of the largest order.
    let largest = find_many::<Order>().order_by(|o| o.amount.desc()).take(1).pluck(|o| o.customer_id);
    assert_eq!(customers(&client, |x| x.id.in_query(largest)).await?, ["c2"]);

    let next_two = find_many::<Order>().order_by(|o| o.amount.desc()).skip(1).take(2).pluck(|o| o.customer_id);
    assert_eq!(customers(&client, |x| x.id.in_query(next_two)).await?, ["c1"]);

    // `find_first` selects at most one value.
    let smallest = find_first::<Order>().order_by(|o| o.amount.asc()).pluck(|o| o.customer_id);
    assert_eq!(customers(&client, |x| x.id.in_query(smallest)).await?, ["c3"]);
    let none = find_first::<Order>().where_(|o| o.amount.gt(10_000_i64)).pluck(|o| o.customer_id);
    assert!(customers(&client, |x| x.id.in_query(none)).await?.is_empty());

    // Without `take`/`skip` the order changes nothing.
    let ordered = find_many::<Customer>().order_by(|c| c.id.desc()).where_(|c| c.vip.eq(true)).pluck(|c| c.id);
    assert_eq!(orders(&client, |x| x.customer_id.in_query(ordered)).await?, ["o1", "o2", "o4"]);
    Ok(())
}

async fn nested_and_relation_filters(client: DinocoClient) -> anyhow::Result<()> {
    // Orders of the customers who have a refunded order.
    let refunded = find_many::<Customer>()
        .where_(|c| c.id.in_query(find_many::<Order>().where_(|o| o.status.eq("refunded")).pluck(|o| o.customer_id)))
        .pluck(|c| c.id);
    assert_eq!(orders(&client, |x| x.customer_id.in_query(refunded)).await?, ["o4"]);

    // A relation filter inside the subquery.
    let paying = find_many::<Customer>().where_(|c| c.orders.where_(|o| o.status.eq("paid"))).pluck(|c| c.id);
    assert_eq!(orders(&client, |x| x.customer_id.in_query(paying)).await?, ["o1", "o2", "o3"]);

    // A subquery inside a relation filter.
    let large = || find_many::<Order>().where_(|o| o.amount.gt(200_i64)).pluck(|o| o.id);
    assert_eq!(customers(&client, |x| x.orders.where_(|o| o.id.in_query(large()))).await?, ["c1", "c2"]);
    assert_eq!(customers(&client, |x| x.orders.none(|o| o.id.in_query(large()))).await?, ["c3", "c4"]);

    // A subquery on the table it filters.
    let above_average = find_many::<Order>().where_(|o| o.amount.gt(150_i64)).pluck(|o| o.id);
    assert_eq!(orders(&client, |x| x.id.in_query(above_average)).await?, ["o2", "o3"]);
    Ok(())
}

async fn where_complex_and_reads(client: DinocoClient) -> anyhow::Result<()> {
    let rows = find_many::<Order>()
        .where_complex(|x, c| c.or(x.customer_id.in_query(vip_ids()), x.status.eq("paid")))
        .order_by(|x| x.id.asc())
        .pluck(|x| x.id)
        .execute(&client)
        .await?;
    assert_eq!(rows, ["o1", "o2", "o3", "o4", "o5"]);
    let rows = find_many::<Order>()
        .where_complex(|x, c| c.and([c.not(x.customer_id.in_query(vip_ids())), x.customer_id.not_null()]))
        .pluck(|x| x.id)
        .execute(&client)
        .await?;
    assert_eq!(rows, ["o3"]);

    assert_eq!(count::<Order>().where_(|x| x.customer_id.in_query(vip_ids())).execute(&client).await?.total, 3);
    assert!(exists::<Order>().where_(|x| x.customer_id.not_in_query(vip_ids())).execute(&client).await?);
    let first = find_first::<Order>()
        .where_(|x| x.customer_id.in_query(vip_ids()))
        .order_by(|x| x.amount.desc())
        .execute(&client)
        .await?;
    assert_eq!(first.map(|order| order.id), Some("o2".to_string()));

    // `remove` drops it from a counting find's count.
    let (rows, count) = find_many::<Order>()
        .where_(|x| x.customer_id.in_query(vip_ids()))
        .count_transform(|count| count.remove(|x| x.customer_id))
        .execute(&client)
        .await?;
    assert_eq!((rows.len(), count.total), (3, 5));

    // In an include's filter.
    let with_orders = find_many::<Customer>()
        .order_by(|x| x.id.asc())
        .includes(|x| {
            x.orders().where_(|o| o.id.in_query(find_many::<Order>().where_(|o| o.amount.gte(100_i64)).pluck(|o| o.id)))
        })
        .execute(&client)
        .await?;
    let included = with_orders.iter().map(|customer| customer.orders.len()).collect::<Vec<_>>();
    assert_eq!(included, [2, 1, 0, 0]);
    Ok(())
}

async fn writes_on_the_same_table(client: DinocoClient) -> anyhow::Result<()> {
    // MySQL rejects a write whose subquery reads the written table; Dinoco
    // reads it from a derived table there.
    update_many::<Order>()
        .where_(|x| x.id.in_query(find_many::<Order>().where_(|o| o.amount.gt(200_i64)).pluck(|o| o.id)))
        .update(|x| x.status.set("flagged"))
        .execute(&client)
        .await?;
    assert_eq!(orders(&client, |x| x.status.eq("flagged")).await?, ["o2", "o3"]);

    delete_many::<Order>()
        .where_(|x| x.id.not_in_query(find_many::<Order>().where_(|o| o.status.eq("paid")).pluck(|o| o.id)))
        .execute(&client)
        .await?;
    assert_eq!(orders(&client, |x| x.amount.gt(0_i64)).await?, ["o1", "o5"]);

    // Across tables too.
    let ordering = find_many::<Order>().where_(|o| o.customer_id.not_null()).pluck(|o| o.customer_id);
    delete_many::<Customer>().where_(|x| x.id.not_in_query(ordering)).execute(&client).await?;
    assert_eq!(customers(&client, |x| x.id.not_null()).await?, ["c1"]);
    Ok(())
}

async fn empty_subqueries(client: DinocoClient) -> anyhow::Result<()> {
    let nobody = || find_many::<Customer>().where_(|c| c.region.eq("east")).pluck(|c| c.id);
    assert!(orders(&client, |x| x.customer_id.in_query(nobody())).await?.is_empty());
    // `NOT IN` an empty list is true for every row, even a `NULL` column.
    assert_eq!(orders(&client, |x| x.customer_id.not_in_query(nobody())).await?, ["o1", "o2", "o3", "o4", "o5"]);
    Ok(())
}

async fn cursors(client: DinocoClient) -> anyhow::Result<()> {
    // The cursor of the subquery picks its rows.
    let later = find_many::<Order>().order_by(|o| o.id.asc()).after(("o3",)).pluck(|o| o.customer_id);
    assert_eq!(customers(&client, |x| x.id.in_query(later)).await?, ["c3"]);
    let earlier = find_many::<Order>().order_by(|o| o.id.asc()).before(("o3",)).take(1).pluck(|o| o.customer_id);
    assert_eq!(customers(&client, |x| x.id.in_query(earlier)).await?, ["c1"]);

    // A cursor without `order_by` can't be applied: the subquery selects nothing.
    let invalid = find_many::<Order>().after(("o3",)).pluck(|o| o.customer_id);
    assert!(customers(&client, |x| x.id.in_query(invalid)).await?.is_empty());
    Ok(())
}

async fn transactions(client: DinocoClient) -> anyhow::Result<()> {
    let rows = transaction(&client, |tx| async move {
        update_many::<Customer>().where_(|x| x.id.eq("c2")).update(|x| x.vip.set(true)).execute(tx).await?;
        Ok(find_many::<Order>()
            .where_(|x| x.customer_id.in_query(vip_ids()))
            .order_by(|x| x.id.asc())
            .pluck(|x| x.id)
            .execute(tx)
            .await?)
    })
    .await?;
    // The subquery sees the transaction's own update.
    assert_eq!(rows, ["o1", "o2", "o3", "o4"]);
    Ok(())
}

/// The statement each database gets, with MySQL's derived tables.
#[tokio::test]
async fn compiles_to_in_subqueries() -> anyhow::Result<()> {
    let sqlite = SqliteAdapter::new(":memory:".to_string()).await.map_err(anyhow::Error::msg)?;
    let postgres = PostgresAdapter::pgbouncer(POSTGRES_URL).await?;
    let mysql = MySqlAdapter::new(MYSQL_URL);
    let query = |subquery: dinoco::Pluck<Customer, String>| {
        let mut query = FindQuery::new(&["id"], "iq_order", -1, -1);
        query.conditions = vec![OrderWhere::default().customer_id.in_query(subquery)];
        query
    };

    let (sql, params) = sqlite.compile_find_query(query(vip_ids()));
    assert_eq!(sql, "SELECT id FROM iq_order WHERE customer_id IN (SELECT id FROM iq_customer WHERE vip = ?)");
    assert_eq!(params, [DinocoValue::Boolean(true)]);
    let (sql, _) = postgres.compile_find_query(query(vip_ids()));
    assert_eq!(sql, "SELECT id FROM iq_order WHERE customer_id IN (SELECT id FROM iq_customer WHERE vip = $1)");

    // A window: MySQL reads it from a derived table.
    let window = || find_many::<Customer>().order_by(|c| c.id.asc()).take(2).pluck(|c| c.id);
    let (sql, params) = mysql.compile_find_query(query(window()));
    assert_eq!(
        sql,
        "SELECT id FROM iq_order WHERE customer_id IN (SELECT id FROM (SELECT id FROM iq_customer ORDER BY id ASC \
         LIMIT ?) AS __dinoco_subquery)"
    );
    assert_eq!(params, [DinocoValue::Integer(2)]);
    let (sql, _) = sqlite.compile_find_query(query(window()));
    assert_eq!(
        sql,
        "SELECT id FROM iq_order WHERE customer_id IN (SELECT id FROM iq_customer ORDER BY id ASC LIMIT ?)"
    );

    // A write reading its own table: MySQL only, a derived table.
    let not_in = FindWhere::InQuery(
        "id",
        Box::new(SubQuery { query: FindQuery::new(&["id"], "iq_order", -1, -1), negated: true }),
    );
    let delete =
        |conditions: Vec<FindWhere>| dinoco_engine::DeleteQuery { table: "iq_order", conditions, returning: None };
    let (sql, _) = mysql.compile_delete_query(delete(vec![not_in.clone()]));
    assert_eq!(
        sql,
        "DELETE FROM iq_order WHERE id NOT IN (SELECT id FROM (SELECT id FROM iq_order) AS __dinoco_subquery)"
    );
    let (sql, _) = postgres.compile_delete_query(delete(vec![not_in]));
    assert_eq!(sql, "DELETE FROM iq_order WHERE id NOT IN (SELECT id FROM iq_order)");
    Ok(())
}

async fn run_on_sqlite<F, Fut>(name: &str, scenario: F) -> anyhow::Result<()>
where
    F: FnOnce(DinocoClient) -> Fut,
    Fut: Future<Output = anyhow::Result<()>>,
{
    let path = format!("/private/tmp/dinoco-iq-{name}-{}-{}.sqlite", std::process::id(), monotonic());
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
    create_table(
        adapter,
        "iq_customer",
        vec![primary(string("id")), column("vip", MigrationColumnType::Boolean), string("region")],
    )
    .await?;
    create_table(
        adapter,
        "iq_order",
        vec![
            primary(string("id")),
            nullable(string("customer_id")),
            column("amount", MigrationColumnType::Integer),
            string("status"),
        ],
    )
    .await
}

fn monotonic() -> u128 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
}
