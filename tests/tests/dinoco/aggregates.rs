//! `find_many(...).pluck(...)` with an aggregate (`sum`, `avg`, `min`, `max`,
//! `count`) and `.group_by(...)` (a field, or the day, week, month or year of
//! a date). Covers `where_`, `where_complex`, relation filters, `order_by`,
//! `take`/`skip` (a window of rows without a group, a page of groups with one),
//! `NULL`, empty results, every value type, enum and optional keys, the
//! `order_by` error path, transactions, hooks and the generated types.
//!
//! Every scenario runs on SQLite, PostgreSQL and MySQL (the same local servers
//! `docker_adapters.rs` uses).

use std::future::Future;
use std::sync::{Arc, Mutex};

use dinoco::chrono::{DateTime, NaiveDate, TimeZone, Utc};
use dinoco::{
    Aggregate, DinocoEnum, Entity, GroupKey, PluckAggregate, PluckGroup, find_first, find_many, insert_into,
    insert_many, remove_test_methods, setup_test_methods, transaction,
};
use dinoco_engine::{
    Backend, CreateEnumMigration, DinocoAdapter, DinocoClient, DinocoSqlCompiler, DropEnumMigration,
    MigrationColumnType, MySqlAdapter, PostgresAdapter, SqliteAdapter,
};
use dinoco_tests::{column, create_table, drop_table, nullable, primary};

const POSTGRES_URL: &str = "postgres://postgres:postgres@localhost:5432/postgres";
const MYSQL_URL: &str = "mysql://root:root@localhost:3306/mysql";
static POSTGRES_TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
static MYSQL_TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

const TABLES: [&str; 2] = ["ag_customer", "ag_order"];
const STATUSES: [&str; 3] = ["paid", "refunded", "pending"];

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
    sums,
    averages,
    minimums_and_maximums,
    counts,
    filters,
    windows,
    groups_by_field,
    groups_by_date,
    groups_by_optional_keys,
    group_order_and_pages,
    group_order_errors,
    empty_tables,
    transactions,
    hooks,
);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, DinocoEnum)]
pub enum AgOrderStatus {
    #[default]
    #[dinoco(value = "paid")]
    Paid,
    #[dinoco(value = "refunded")]
    Refunded,
    #[dinoco(value = "pending")]
    Pending,
}

#[derive(Debug, Entity)]
#[dinoco(table_name = "ag_customer")]
pub struct Customer {
    #[dinoco(primary_key)]
    id: String,
    vip: bool,

    #[dinoco(one_to_many, foreign_key = "customer_id", references = "id")]
    orders: Vec<Order>,
}

#[derive(Debug, Entity)]
#[dinoco(table_name = "ag_order")]
pub struct Order {
    #[dinoco(primary_key)]
    id: String,
    customer_id: Option<String>,
    status: AgOrderStatus,
    amount: i64,
    discount: Option<i64>,
    ratio: f64,
    tip: Option<f64>,
    note: Option<String>,
    created_at: DateTime<Utc>,
    shipped_at: Option<DateTime<Utc>>,
    due_on: NaiveDate,

    #[dinoco(many_to_one, foreign_key = "customer_id", references = "id")]
    customer: Option<Customer>,
}

fn at(year: i32, month: u32, day: u32, hour: u32, minute: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(year, month, day, hour, minute, 0).unwrap()
}

fn date(year: i32, month: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(year, month, day).unwrap()
}

fn close(left: f64, right: f64) -> bool {
    (left - right).abs() < 1e-9
}

/// | order | customer | status   | amount | discount | ratio | tip  | note | created_at (UTC)       | shipped_at | due_on     |
/// | ----- | -------- | -------- | ------ | -------- | ----- | ---- | ---- | ---------------------- | ---------- | ---------- |
/// | o1    | ana      | paid     | 100    | 10       | 0.5   | 1.5  | a    | 2026-01-05 09:00 (Mon) | 2026-01-06 | 2026-01-10 |
/// | o2    | ana      | paid     | 250    |          | 1.5   |      |      | 2026-01-05 18:30 (Mon) |            | 2026-01-10 |
/// | o3    | bia      | refunded | 400    | 40       | 2.0   | 2.5  | c    | 2026-01-07 12:00 (Wed) | 2026-01-08 | 2026-02-01 |
/// | o4    | bia      | paid     | 50     |          | 0.25  |      | d    | 2026-01-11 23:59 (Sun) |            | 2026-02-15 |
/// | o5    |          | pending  | 75     | 5        | 1.0   | 0.5  |      | 2026-02-03 08:00 (Tue) |            | 2027-03-01 |
/// | o6    | caio     | paid     | 125    |          | 0.75  |      |      | 2025-12-31 23:00 (Wed) | 2026-01-02 | 2025-12-31 |
///
/// Customers: ana and caio are VIP, bia is not, duda has no orders.
async fn seed(client: &DinocoClient) -> anyhow::Result<()> {
    let customer = |id: &str, vip: bool| Customer { id: id.to_string(), vip, orders: Vec::new() };
    insert_many::<Customer>()
        .values([customer("ana", true), customer("bia", false), customer("caio", true), customer("duda", false)])
        .execute(client)
        .await?;

    #[allow(clippy::too_many_arguments)]
    let order = |id: &str,
                 customer_id: Option<&str>,
                 status: AgOrderStatus,
                 amount: i64,
                 discount: Option<i64>,
                 ratio: f64,
                 tip: Option<f64>,
                 note: Option<&str>,
                 created_at: DateTime<Utc>,
                 shipped_at: Option<DateTime<Utc>>,
                 due_on: NaiveDate| Order {
        id: id.to_string(),
        customer_id: customer_id.map(str::to_string),
        status,
        amount,
        discount,
        ratio,
        tip,
        note: note.map(str::to_string),
        created_at,
        shipped_at,
        due_on,
        customer: None,
    };
    use AgOrderStatus::{Paid, Pending, Refunded};
    insert_many::<Order>()
        .values([
            order(
                "o1",
                Some("ana"),
                Paid,
                100,
                Some(10),
                0.5,
                Some(1.5),
                Some("a"),
                at(2026, 1, 5, 9, 0),
                Some(at(2026, 1, 6, 10, 0)),
                date(2026, 1, 10),
            ),
            order("o2", Some("ana"), Paid, 250, None, 1.5, None, None, at(2026, 1, 5, 18, 30), None, date(2026, 1, 10)),
            order(
                "o3",
                Some("bia"),
                Refunded,
                400,
                Some(40),
                2.0,
                Some(2.5),
                Some("c"),
                at(2026, 1, 7, 12, 0),
                Some(at(2026, 1, 8, 10, 0)),
                date(2026, 2, 1),
            ),
            order(
                "o4",
                Some("bia"),
                Paid,
                50,
                None,
                0.25,
                None,
                Some("d"),
                at(2026, 1, 11, 23, 59),
                None,
                date(2026, 2, 15),
            ),
            order("o5", None, Pending, 75, Some(5), 1.0, Some(0.5), None, at(2026, 2, 3, 8, 0), None, date(2027, 3, 1)),
            order(
                "o6",
                Some("caio"),
                Paid,
                125,
                None,
                0.75,
                None,
                None,
                at(2025, 12, 31, 23, 0),
                Some(at(2026, 1, 2, 10, 0)),
                date(2025, 12, 31),
            ),
        ])
        .execute(client)
        .await?;
    Ok(())
}

async fn sums(client: DinocoClient) -> anyhow::Result<()> {
    let total: i64 = find_many::<Order>().pluck(|x| x.amount.sum()).execute(&client).await?;
    assert_eq!(total, 1000);

    let paid = find_many::<Order>()
        .where_(|x| x.status.eq(AgOrderStatus::Paid))
        .pluck(|x| x.amount.sum())
        .execute(&client)
        .await?;
    assert_eq!(paid, 525);

    // `NULL`s are skipped, and an optional field still sums to a plain value.
    let discounts: i64 = find_many::<Order>().pluck(|x| x.discount.sum()).execute(&client).await?;
    assert_eq!(discounts, 55);
    let no_discount =
        find_many::<Order>().where_(|x| x.discount.null()).pluck(|x| x.discount.sum()).execute(&client).await?;
    assert_eq!(no_discount, 0);

    let ratios: f64 = find_many::<Order>().pluck(|x| x.ratio.sum()).execute(&client).await?;
    assert!(close(ratios, 6.0), "{ratios}");
    let tips: f64 = find_many::<Order>().pluck(|x| x.tip.sum()).execute(&client).await?;
    assert!(close(tips, 4.5), "{tips}");

    // No row matches: a sum is zero, not missing.
    let none = find_many::<Order>().where_(|x| x.amount.gt(10_000)).pluck(|x| x.amount.sum()).execute(&client).await?;
    assert_eq!(none, 0);
    let none = find_many::<Order>().where_(|x| x.amount.gt(10_000)).pluck(|x| x.ratio.sum()).execute(&client).await?;
    assert!(close(none, 0.0), "{none}");

    // `read_in_primary()` is accepted before and after `.pluck(...)`.
    let total = find_many::<Order>().read_in_primary().pluck(|x| x.amount.sum()).read_in_primary();
    assert_eq!(total.execute(&client).await?, 1000);
    Ok(())
}

async fn averages(client: DinocoClient) -> anyhow::Result<()> {
    let paid = find_many::<Order>()
        .where_(|x| x.status.eq(AgOrderStatus::Paid))
        .pluck(|x| x.amount.avg())
        .execute(&client)
        .await?;
    assert!(paid.is_some_and(|average| close(average, 131.25)), "{paid:?}");

    let all = find_many::<Order>().pluck(|x| x.amount.avg()).execute(&client).await?;
    assert!(all.is_some_and(|average| close(average, 1000.0 / 6.0)), "{all:?}");

    let discounts = find_many::<Order>().pluck(|x| x.discount.avg()).execute(&client).await?;
    assert!(discounts.is_some_and(|average| close(average, 55.0 / 3.0)), "{discounts:?}");
    let ratios = find_many::<Order>().pluck(|x| x.ratio.avg()).execute(&client).await?;
    assert!(ratios.is_some_and(|average| close(average, 1.0)), "{ratios:?}");

    // Only `NULL`, or no row at all: no average.
    let only_null = find_many::<Order>().where_(|x| x.tip.null()).pluck(|x| x.tip.avg()).execute(&client).await?;
    assert_eq!(only_null, None);
    let empty = find_many::<Order>().where_(|x| x.amount.gt(10_000)).pluck(|x| x.amount.avg()).execute(&client).await?;
    assert_eq!(empty, None);
    Ok(())
}

async fn minimums_and_maximums(client: DinocoClient) -> anyhow::Result<()> {
    let orders = || find_many::<Order>();
    assert_eq!(orders().pluck(|x| x.amount.min()).execute(&client).await?, Some(50));
    assert_eq!(orders().pluck(|x| x.amount.max()).execute(&client).await?, Some(400));
    assert_eq!(orders().pluck(|x| x.discount.min()).execute(&client).await?, Some(5));
    assert_eq!(orders().pluck(|x| x.ratio.max()).execute(&client).await?, Some(2.0));
    assert_eq!(orders().pluck(|x| x.tip.min()).execute(&client).await?, Some(0.5));
    assert_eq!(orders().pluck(|x| x.note.min()).execute(&client).await?.as_deref(), Some("a"));
    assert_eq!(orders().pluck(|x| x.note.max()).execute(&client).await?.as_deref(), Some("d"));
    assert_eq!(orders().pluck(|x| x.created_at.min()).execute(&client).await?, Some(at(2025, 12, 31, 23, 0)));
    assert_eq!(orders().pluck(|x| x.created_at.max()).execute(&client).await?, Some(at(2026, 2, 3, 8, 0)));
    assert_eq!(orders().pluck(|x| x.shipped_at.min()).execute(&client).await?, Some(at(2026, 1, 2, 10, 0)));
    assert_eq!(orders().pluck(|x| x.due_on.min()).execute(&client).await?, Some(date(2025, 12, 31)));
    assert_eq!(orders().pluck(|x| x.due_on.max()).execute(&client).await?, Some(date(2027, 3, 1)));

    let refunded = orders().where_(|x| x.status.eq(AgOrderStatus::Refunded));
    assert_eq!(refunded.pluck(|x| x.amount.max()).execute(&client).await?, Some(400));

    let empty = || orders().where_(|x| x.amount.gt(10_000));
    assert_eq!(empty().pluck(|x| x.amount.min()).execute(&client).await?, None);
    assert_eq!(empty().pluck(|x| x.note.max()).execute(&client).await?, None);
    assert_eq!(empty().pluck(|x| x.created_at.max()).execute(&client).await?, None);
    Ok(())
}

async fn counts(client: DinocoClient) -> anyhow::Result<()> {
    assert_eq!(find_many::<Order>().pluck(|x| x.id.count()).execute(&client).await?, 6);
    // `count` skips `NULL`.
    assert_eq!(find_many::<Order>().pluck(|x| x.discount.count()).execute(&client).await?, 3);
    assert_eq!(find_many::<Order>().pluck(|x| x.shipped_at.count()).execute(&client).await?, 3);
    assert_eq!(find_many::<Order>().pluck(|x| x.status.count()).execute(&client).await?, 6);
    let none = find_many::<Order>().where_(|x| x.amount.gt(10_000)).pluck(|x| x.id.count()).execute(&client).await?;
    assert_eq!(none, 0);
    Ok(())
}

async fn filters(client: DinocoClient) -> anyhow::Result<()> {
    // Repeated `where_` calls combine with `AND`.
    let total = find_many::<Order>()
        .where_(|x| x.status.eq(AgOrderStatus::Paid))
        .where_(|x| x.amount.gte(100))
        .pluck(|x| x.amount.sum())
        .execute(&client)
        .await?;
    assert_eq!(total, 475);

    let total = find_many::<Order>()
        .where_complex(|x, c| c.or(x.status.eq(AgOrderStatus::Refunded), x.amount.lt(60)))
        .pluck(|x| x.amount.sum())
        .execute(&client)
        .await?;
    assert_eq!(total, 450);

    // `where_complex` replaces the plain `where_` calls before it.
    let total = find_many::<Order>()
        .where_(|x| x.status.eq(AgOrderStatus::Pending))
        .where_complex(|x, c| c.not(x.customer_id.null()))
        .pluck(|x| x.amount.sum())
        .execute(&client)
        .await?;
    assert_eq!(total, 925);

    // Relation filters narrow the aggregated rows by their related rows.
    let vip = find_many::<Order>()
        .where_(|x| x.customer.where_(|customer| customer.vip.eq(true)))
        .pluck(|x| x.amount.sum())
        .execute(&client)
        .await?;
    assert_eq!(vip, 475);
    let by_vip_day = find_many::<Order>()
        .where_(|x| x.customer.where_(|customer| customer.vip.eq(true)))
        .pluck(|x| x.amount.sum())
        .group_by(|x| x.created_at.day())
        .execute(&client)
        .await?;
    assert_eq!(by_vip_day, [(date(2025, 12, 31), 125), (date(2026, 1, 5), 350)]);

    let in_batch = find_many::<Order>()
        .where_(|x| x.id.batch(["o1", "o3", "missing"]))
        .pluck(|x| x.amount.sum())
        .execute(&client)
        .await?;
    assert_eq!(in_batch, 500);
    Ok(())
}

async fn windows(client: DinocoClient) -> anyhow::Result<()> {
    // `order_by` alone changes nothing: every row is aggregated.
    let total = find_many::<Order>().order_by(|x| x.amount.desc()).pluck(|x| x.amount.sum()).execute(&client).await?;
    assert_eq!(total, 1000);

    // `take` and `skip` aggregate only the rows `find_many` would return.
    let top_three =
        find_many::<Order>().order_by(|x| x.amount.desc()).take(3).pluck(|x| x.amount.sum()).execute(&client).await?;
    assert_eq!(top_three, 775);

    let second_and_third = find_many::<Order>()
        .order_by(|x| x.created_at.asc())
        .skip(1)
        .take(2)
        .pluck(|x| x.amount.sum())
        .execute(&client)
        .await?;
    assert_eq!(second_and_third, 350);

    let counted = find_many::<Order>()
        .where_(|x| x.status.eq(AgOrderStatus::Paid))
        .order_by(|x| x.amount.asc())
        .take(2)
        .pluck(|x| x.id.count())
        .execute(&client)
        .await?;
    assert_eq!(counted, 2);

    let cheapest_paid = find_many::<Order>()
        .where_(|x| x.status.eq(AgOrderStatus::Paid))
        .order_by(|x| x.amount.asc())
        .take(2)
        .pluck(|x| x.amount.max())
        .execute(&client)
        .await?;
    assert_eq!(cheapest_paid, Some(100));

    let nothing = find_many::<Order>().take(0).pluck(|x| x.amount.sum()).execute(&client).await?;
    assert_eq!(nothing, 0);
    Ok(())
}

async fn groups_by_field(client: DinocoClient) -> anyhow::Result<()> {
    // Enum keys sort differently in every database (declaration order for
    // PostgreSQL and MySQL enums, text for SQLite), so order by the value.
    let by_status = find_many::<Order>()
        .order_by(|x| x.amount.desc())
        .pluck(|x| x.amount.sum())
        .group_by(|x| x.status)
        .execute(&client)
        .await?;
    assert_eq!(by_status, [(AgOrderStatus::Paid, 525), (AgOrderStatus::Refunded, 400), (AgOrderStatus::Pending, 75)]);

    // Enum fields pluck as their own type too, in every builder.
    let statuses = find_many::<Order>()
        .where_(|x| x.id.batch(["o3", "o5"]))
        .order_by(|x| x.id.asc())
        .pluck(|x| x.status)
        .execute(&client)
        .await?;
    assert_eq!(statuses, [AgOrderStatus::Refunded, AgOrderStatus::Pending]);
    let first = find_first::<Order>().where_(|x| x.id.eq("o1")).pluck(|x| x.status).execute(&client).await?;
    assert_eq!(first, Some(AgOrderStatus::Paid));

    let mut counts = find_many::<Order>().pluck(|x| x.id.count()).group_by(|x| x.status).execute(&client).await?;
    counts.sort();
    assert_eq!(counts, [(AgOrderStatus::Paid, 4), (AgOrderStatus::Refunded, 1), (AgOrderStatus::Pending, 1)]);

    let by_amount = find_many::<Order>()
        .where_(|x| x.amount.lte(100))
        .pluck(|x| x.ratio.sum())
        .group_by(|x| x.amount)
        .execute(&client)
        .await?;
    assert_eq!(by_amount, [(50, 0.25), (75, 1.0), (100, 0.5)]);

    let averages = find_many::<Order>()
        .where_(|x| x.customer_id.not_null())
        .pluck(|x| x.amount.avg())
        .group_by(|x| x.customer_id)
        .execute(&client)
        .await?;
    assert_eq!(
        averages,
        [
            (Some("ana".to_string()), Some(175.0)),
            (Some("bia".to_string()), Some(225.0)),
            (Some("caio".to_string()), Some(125.0))
        ]
    );

    let latest = find_many::<Order>()
        .where_(|x| x.customer_id.not_null())
        .pluck(|x| x.created_at.max())
        .group_by(|x| x.customer_id)
        .execute(&client)
        .await?;
    assert_eq!(latest[0], (Some("ana".to_string()), Some(at(2026, 1, 5, 18, 30))));
    Ok(())
}

async fn groups_by_date(client: DinocoClient) -> anyhow::Result<()> {
    let sum_by = |key: fn(OrderWhere) -> GroupKey<NaiveDate>| {
        let client = &client;
        async move { find_many::<Order>().pluck(|x| x.amount.sum()).group_by(key).execute(client).await }
    };

    assert_eq!(
        sum_by(|x| x.created_at.day()).await?,
        [
            (date(2025, 12, 31), 125),
            (date(2026, 1, 5), 350),
            (date(2026, 1, 7), 400),
            (date(2026, 1, 11), 50),
            (date(2026, 2, 3), 75),
        ]
    );
    // Weeks start on Monday: Sunday the 11th belongs to the week of the 5th.
    assert_eq!(
        sum_by(|x| x.created_at.week()).await?,
        [(date(2025, 12, 29), 125), (date(2026, 1, 5), 800), (date(2026, 2, 2), 75)]
    );
    assert_eq!(
        sum_by(|x| x.created_at.month()).await?,
        [(date(2025, 12, 1), 125), (date(2026, 1, 1), 800), (date(2026, 2, 1), 75)]
    );
    assert_eq!(sum_by(|x| x.created_at.year()).await?, [(date(2025, 1, 1), 125), (date(2026, 1, 1), 875)]);

    // A `Date` field groups the same way.
    assert_eq!(
        sum_by(|x| x.due_on.month()).await?,
        [(date(2025, 12, 1), 125), (date(2026, 1, 1), 350), (date(2026, 2, 1), 450), (date(2027, 3, 1), 75)]
    );
    assert_eq!(sum_by(|x| x.due_on.day()).await?[0], (date(2025, 12, 31), 125));

    let paid_by_month = find_many::<Order>()
        .where_(|x| x.status.eq(AgOrderStatus::Paid))
        .pluck(|x| x.amount.sum())
        .group_by(|x| x.created_at.month())
        .execute(&client)
        .await?;
    assert_eq!(paid_by_month, [(date(2025, 12, 1), 125), (date(2026, 1, 1), 400)]);

    let ratio_by_year =
        find_many::<Order>().pluck(|x| x.ratio.sum()).group_by(|x| x.created_at.year()).execute(&client).await?;
    assert_eq!(ratio_by_year, [(date(2025, 1, 1), 0.75), (date(2026, 1, 1), 5.25)]);

    let cheapest_by_day =
        find_many::<Order>().pluck(|x| x.amount.min()).group_by(|x| x.created_at.day()).execute(&client).await?;
    assert_eq!(cheapest_by_day[1], (date(2026, 1, 5), Some(100)));
    Ok(())
}

async fn groups_by_optional_keys(client: DinocoClient) -> anyhow::Result<()> {
    // `NULL` is a group of its own; where it sorts depends on the database.
    let mut by_customer =
        find_many::<Order>().pluck(|x| x.id.count()).group_by(|x| x.customer_id).execute(&client).await?;
    by_customer.sort();
    assert_eq!(
        by_customer,
        [(None, 1), (Some("ana".to_string()), 2), (Some("bia".to_string()), 2), (Some("caio".to_string()), 1)]
    );

    let mut by_shipping_day =
        find_many::<Order>().pluck(|x| x.amount.sum()).group_by(|x| x.shipped_at.day()).execute(&client).await?;
    by_shipping_day.sort();
    assert_eq!(
        by_shipping_day,
        [(None, 375), (Some(date(2026, 1, 2)), 125), (Some(date(2026, 1, 6)), 100), (Some(date(2026, 1, 8)), 400)]
    );

    let mut tips_by_note = find_many::<Order>().pluck(|x| x.tip.sum()).group_by(|x| x.note).execute(&client).await?;
    tips_by_note.sort_by(|left, right| left.0.cmp(&right.0));
    assert_eq!(
        tips_by_note,
        [(None, 0.5), (Some("a".to_string()), 1.5), (Some("c".to_string()), 2.5), (Some("d".to_string()), 0.0)]
    );
    Ok(())
}

async fn group_order_and_pages(client: DinocoClient) -> anyhow::Result<()> {
    let by_day = || find_many::<Order>();

    // `order_by` on the grouped field orders the groups by key.
    let newest_days = by_day()
        .order_by(|x| x.created_at.desc())
        .take(2)
        .pluck(|x| x.amount.sum())
        .group_by(|x| x.created_at.day())
        .execute(&client)
        .await?;
    assert_eq!(newest_days, [(date(2026, 2, 3), 75), (date(2026, 1, 11), 50)]);

    // On the aggregated field, by value; `take` and `skip` page the groups.
    let best_days = by_day()
        .order_by(|x| x.amount.desc())
        .skip(1)
        .take(2)
        .pluck(|x| x.amount.sum())
        .group_by(|x| x.created_at.day())
        .execute(&client)
        .await?;
    assert_eq!(best_days, [(date(2026, 1, 5), 350), (date(2025, 12, 31), 125)]);

    let worst_days = by_day()
        .order_by(|x| x.amount.asc())
        .take(1)
        .pluck(|x| x.amount.sum())
        .group_by(|x| x.created_at.day())
        .execute(&client)
        .await?;
    assert_eq!(worst_days, [(date(2026, 1, 11), 50)]);

    // Ties on the value are broken by the key, ascending.
    let busiest = by_day()
        .order_by(|x| x.id.desc())
        .pluck(|x| x.id.count())
        .group_by(|x| x.created_at.day())
        .execute(&client)
        .await?;
    assert_eq!(
        busiest,
        [
            (date(2026, 1, 5), 2),
            (date(2025, 12, 31), 1),
            (date(2026, 1, 7), 1),
            (date(2026, 1, 11), 1),
            (date(2026, 2, 3), 1),
        ]
    );

    // Without `order_by`, groups come by key, ascending, and can still be paged.
    let page =
        by_day().skip(3).take(5).pluck(|x| x.amount.sum()).group_by(|x| x.created_at.day()).execute(&client).await?;
    assert_eq!(page, [(date(2026, 1, 11), 50), (date(2026, 2, 3), 75)]);

    let past_the_end =
        by_day().skip(10).take(5).pluck(|x| x.amount.sum()).group_by(|x| x.created_at.day()).execute(&client).await?;
    assert!(past_the_end.is_empty());
    Ok(())
}

async fn group_order_errors(client: DinocoClient) -> anyhow::Result<()> {
    let error = find_many::<Order>()
        .order_by(|x| x.ratio.desc())
        .pluck(|x| x.amount.sum())
        .group_by(|x| x.created_at.day())
        .execute(&client)
        .await
        .expect_err("groups have no ratio column to order by");
    assert_eq!(
        error.to_string(),
        "a grouped pluck on `ag_order` can only be ordered by the grouped or the aggregated field, not by `ratio`"
    );

    // The same `order_by` is fine without a group: it picks the window.
    let total =
        find_many::<Order>().order_by(|x| x.ratio.desc()).take(1).pluck(|x| x.amount.sum()).execute(&client).await?;
    assert_eq!(total, 400);
    Ok(())
}

async fn empty_tables(client: DinocoClient) -> anyhow::Result<()> {
    dinoco::delete_many::<Order>().execute(&client).await?;

    assert_eq!(find_many::<Order>().pluck(|x| x.amount.sum()).execute(&client).await?, 0);
    assert!(close(find_many::<Order>().pluck(|x| x.tip.sum()).execute(&client).await?, 0.0));
    assert_eq!(find_many::<Order>().pluck(|x| x.amount.avg()).execute(&client).await?, None);
    assert_eq!(find_many::<Order>().pluck(|x| x.due_on.max()).execute(&client).await?, None);
    assert_eq!(find_many::<Order>().pluck(|x| x.id.count()).execute(&client).await?, 0);

    let groups =
        find_many::<Order>().pluck(|x| x.amount.sum()).group_by(|x| x.created_at.day()).execute(&client).await?;
    assert!(groups.is_empty());
    Ok(())
}

async fn transactions(client: DinocoClient) -> anyhow::Result<()> {
    let result = transaction(&client, |tx| async move {
        let order = Order {
            id: "o7".to_string(),
            customer_id: None,
            status: AgOrderStatus::Pending,
            amount: 300,
            discount: None,
            ratio: 1.0,
            tip: None,
            note: None,
            created_at: at(2026, 2, 3, 20, 0),
            shipped_at: None,
            due_on: date(2026, 3, 1),
            customer: None,
        };
        insert_into::<Order>().values(&order).execute(tx).await?;

        // Reads inside the closure see its uncommitted write.
        assert_eq!(find_many::<Order>().pluck(|x| x.amount.sum()).execute(tx).await?, 1300);
        let february = find_many::<Order>()
            .where_(|x| x.created_at.gte(at(2026, 2, 1, 0, 0)))
            .pluck(|x| x.amount.sum())
            .group_by(|x| x.created_at.day())
            .execute(tx)
            .await?;
        assert_eq!(february, [(date(2026, 2, 3), 375)]);

        Err::<(), _>(anyhow::anyhow!("roll back"))
    })
    .await;
    assert!(result.is_err());

    assert_eq!(find_many::<Order>().pluck(|x| x.amount.sum()).execute(&client).await?, 1000);

    let committed = transaction(&client, |tx| async move {
        find_many::<Order>()
            .pluck(|x| x.amount.max())
            .group_by(|x| x.status)
            .execute(tx)
            .await
            .map(|groups| groups.len())
    })
    .await?;
    assert_eq!(committed, 3);
    Ok(())
}

async fn hooks(client: DinocoClient) -> anyhow::Result<()> {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&seen);
    setup_test_methods::<Order>(&client).on_find(move |rows, query, error| {
        sink.lock().unwrap().push((rows, query.sql.clone(), query.in_transaction, error.is_some()));
    });

    find_many::<Order>().pluck(|x| x.amount.sum()).execute(&client).await?;
    find_many::<Order>().pluck(|x| x.id.count()).group_by(|x| x.created_at.week()).execute(&client).await?;
    transaction(&client, |tx| async move {
        find_many::<Order>().pluck(|x| x.amount.sum()).group_by(|x| x.status).execute(tx).await?;
        Ok(())
    })
    .await?;
    remove_test_methods::<Order>(&client);
    find_many::<Order>().pluck(|x| x.amount.sum()).execute(&client).await?;

    let seen = seen.lock().unwrap();
    assert_eq!(seen.len(), 3, "{seen:?}");
    assert_eq!(seen[0].0, Some(1));
    assert!(seen[0].1.contains("SUM(amount)"), "{}", seen[0].1);
    assert_eq!(seen[1].0, Some(3));
    assert!(seen[1].1.contains("GROUP BY __dinoco_group_key"), "{}", seen[1].1);
    assert_eq!((seen[2].0, seen[2].2), (Some(3), true));
    assert!(seen.iter().all(|(_, _, _, failed)| !failed));
    Ok(())
}

/// The types `.pluck(...)` and `.group_by(...)` produce.
#[test]
fn aggregate_types() {
    let where_ = OrderWhere::default();
    let _: Aggregate<i64> = where_.amount.sum();
    let _: Aggregate<i64> = where_.discount.sum();
    let _: Aggregate<f64> = where_.tip.sum();
    let _: Aggregate<Option<f64>> = where_.amount.avg();
    let _: Aggregate<Option<i64>> = where_.discount.max();
    let _: Aggregate<Option<String>> = where_.note.min();
    let _: Aggregate<Option<DateTime<Utc>>> = where_.shipped_at.max();
    let _: Aggregate<Option<NaiveDate>> = where_.due_on.min();
    let _: Aggregate<i64> = where_.status.count();
    let _: GroupKey<NaiveDate> = where_.created_at.week();
    let _: GroupKey<Option<NaiveDate>> = where_.shipped_at.month();
    let _: GroupKey<NaiveDate> = where_.due_on.year();

    let _: PluckAggregate<Order, i64> = find_many::<Order>().pluck(|x| x.amount.sum());
    let _: PluckGroup<Order, AgOrderStatus, Option<f64>> =
        find_many::<Order>().pluck(|x| x.ratio.avg()).group_by(|x| x.status);
    let _: PluckGroup<Order, Option<NaiveDate>, i64> =
        find_many::<Order>().pluck(|x| x.id.count()).group_by(|x| x.shipped_at.day());

    // A plain field still plucks the column itself.
    let _: dinoco::Pluck<Order, Option<i64>> = find_many::<Order>().pluck(|x| x.discount);
}

async fn run_on_sqlite<F, Fut>(name: &str, scenario: F) -> anyhow::Result<()>
where
    F: FnOnce(DinocoClient) -> Fut,
    Fut: Future<Output = anyhow::Result<()>>,
{
    let path = format!("/private/tmp/dinoco-ag-{name}-{}-{}.sqlite", std::process::id(), monotonic());
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
    // Only PostgreSQL has named enum types; the other dialects compile these to nothing.
    let enum_statements = adapter
        .compile_drop_enum_migration(DropEnumMigration { name: "AgOrderStatus".to_string() })
        .into_iter()
        .chain(adapter.compile_create_enum_migration(CreateEnumMigration {
            name: "AgOrderStatus".to_string(),
            values: STATUSES.map(str::to_string).to_vec(),
        }));
    for statement in enum_statements {
        adapter.execute(&statement, &[]).await?;
    }

    let string = |name: &str| column(name, MigrationColumnType::String);
    create_table(adapter, "ag_customer", vec![primary(string("id")), column("vip", MigrationColumnType::Boolean)])
        .await?;
    create_table(
        adapter,
        "ag_order",
        vec![
            primary(string("id")),
            nullable(string("customer_id")),
            column(
                "status",
                MigrationColumnType::Enum {
                    name: "AgOrderStatus".to_string(),
                    values: STATUSES.map(str::to_string).to_vec(),
                },
            ),
            column("amount", MigrationColumnType::Integer),
            nullable(column("discount", MigrationColumnType::Integer)),
            column("ratio", MigrationColumnType::Float),
            nullable(column("tip", MigrationColumnType::Float)),
            nullable(string("note")),
            column("created_at", MigrationColumnType::DateTime),
            nullable(column("shipped_at", MigrationColumnType::DateTime)),
            column("due_on", MigrationColumnType::Date),
        ],
    )
    .await
}

fn monotonic() -> u128 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
}
