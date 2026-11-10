//! Aggregate includes: `.includes(|x| x.transactions().pluck(|t| t.amount.sum())
//! .group_by(|t| t.kind))` keeps the parent as it is and fills the relation
//! with one row per group, holding the key and the aggregate in their own
//! fields. Covers grouped and ungrouped aggregates, every aggregate function,
//! date and hour buckets, nullable keys, `where_`/`where_complex`, per-parent
//! `order_by`/`take`/`skip` of the groups, parents without related rows,
//! one-to-many and many-to-many relations, nesting, runtime structs,
//! transactions and the error paths.
//!
//! Every scenario runs on SQLite, PostgreSQL and MySQL (the same local servers
//! `docker_adapters.rs` uses).

use std::future::Future;

use dinoco::chrono::{DateTime, NaiveDate, TimeZone, Utc};
use dinoco::serde::{Deserialize, Serialize};
use dinoco::{
    DinocoRuntimeStruct, DinocoValue, Entity, EntityExtend, InsertQuery, find_many, insert_many, transaction,
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

const TABLES: [&str; 5] = ["gi_business", "gi_account", "gi_transaction", "gi_tag", "_gi_account_to_gi_tag"];

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
    sums_grouped_by_key,
    ungrouped_aggregates,
    groups_are_ordered_and_paged_per_parent,
    filters_pick_the_rows,
    every_aggregate_function,
    date_and_hour_buckets,
    nullable_keys,
    many_to_many,
    nested_includes,
    runtime_structs,
    several_aggregates,
    projections,
    inside_transactions,
    errors,
);

/// Loyalty points, a newtype over an integer column.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, DinocoRuntimeStruct)]
#[serde(crate = "dinoco::serde")]
pub struct Points(i64);

impl From<i64> for Points {
    fn from(points: i64) -> Self {
        Points(points)
    }
}

#[derive(Debug, Clone, Entity)]
#[dinoco(table_name = "gi_business")]
pub struct Business {
    #[dinoco(primary_key)]
    id: String,

    #[dinoco(one_to_many, foreign_key = "business_id", references = "id")]
    accounts: Vec<Account>,
}

#[derive(Debug, Clone, Entity)]
#[dinoco(table_name = "gi_account")]
pub struct Account {
    #[dinoco(primary_key)]
    id: String,
    business_id: String,
    name: String,

    #[dinoco(many_to_one, foreign_key = "business_id", references = "id")]
    business: Option<Box<Business>>,

    #[dinoco(one_to_many, foreign_key = "account_id", references = "id")]
    transactions: Vec<Transaction>,

    #[dinoco(
        many_to_many,
        foreign_key = "id",
        references = "id",
        join_table = "_gi_account_to_gi_tag",
        parent_field = "id",
        join_parent_field = "account_id",
        join_child_field = "tag_id"
    )]
    tags: Vec<Tag>,
}

#[derive(Debug, Clone, Entity)]
#[dinoco(table_name = "gi_transaction")]
pub struct Transaction {
    #[dinoco(primary_key)]
    id: String,
    account_id: Option<String>,
    kind: String,
    status: String,
    amount: i64,
    fee: f64,
    note: Option<String>,
    #[dinoco(scalar)]
    points: Points,
    created_at: DateTime<Utc>,
    settled_on: Option<NaiveDate>,

    #[dinoco(many_to_one, foreign_key = "account_id", references = "id")]
    account: Option<Box<Account>>,
}

/// A group row holding only what a grouped include fills.
#[derive(Debug, Clone, Default, PartialEq, EntityExtend)]
#[extend(Transaction)]
pub struct TransactionTotals {
    pub kind: String,
    pub amount: i64,
    pub fee: f64,
}

/// A row without the aggregated fields.
#[derive(Debug, Clone, Default, EntityExtend)]
#[extend(Transaction)]
pub struct TransactionKind {
    pub kind: String,
}

#[derive(Debug, EntityExtend)]
#[extend(Account)]
pub struct AccountTotals {
    pub id: String,
    pub name: String,
    pub transactions: Vec<TransactionTotals>,
}

#[derive(Debug, EntityExtend)]
#[extend(Account)]
pub struct AccountKinds {
    pub id: String,
    pub transactions: Vec<TransactionKind>,
}

/// Projections to insert an account with its transactions through.
#[derive(Debug, Clone, Default, EntityExtend)]
#[extend(Transaction)]
pub struct NewTransaction {
    pub id: String,
    pub kind: String,
    pub amount: i64,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, EntityExtend)]
#[extend(Account)]
pub struct NewAccount {
    pub id: String,
    pub name: String,
    pub transactions: Vec<NewTransaction>,
}

/// A projection of the parent that keeps the model's own relation type.
#[derive(Debug, EntityExtend)]
#[extend(Account)]
pub struct AccountName {
    pub id: String,
    pub transactions: Vec<Transaction>,
}

#[derive(Debug, Clone, Entity)]
#[dinoco(table_name = "gi_tag")]
pub struct Tag {
    #[dinoco(primary_key)]
    id: String,
    name: String,
    weight: i64,
}

fn at(day: u32, hour: u32, minute: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 3, day, hour, minute, 0).single().expect("valid instant")
}

fn midnight(day: u32) -> DateTime<Utc> {
    at(day, 0, 0)
}

/// `(id, account, kind, status, amount, fee, note, created_at, settled_on day)`.
#[allow(clippy::type_complexity)]
const TRANSACTIONS: [(&str, Option<&str>, &str, &str, i64, f64, Option<&str>, (u32, u32, u32), Option<u32>); 7] = [
    ("t1", Some("a1"), "pix", "paid", 100, 1.0, None, (1, 10, 15), Some(1)),
    ("t2", Some("a1"), "pix", "failed", 50, 0.5, Some("retry"), (1, 10, 45), None),
    ("t3", Some("a1"), "card", "paid", 200, 2.0, None, (2, 9, 0), Some(2)),
    ("t4", Some("a1"), "boleto", "paid", 30, 0.0, Some("late"), (3, 12, 0), Some(5)),
    ("t5", Some("a2"), "card", "paid", 70, 0.75, None, (1, 11, 0), Some(1)),
    ("t6", Some("a2"), "card", "paid", 80, 0.25, Some("late"), (2, 11, 30), Some(1)),
    ("t7", None, "pix", "paid", 999, 9.0, None, (4, 8, 0), None),
];

async fn seed(client: &DinocoClient) -> anyhow::Result<()> {
    insert_many::<Business>()
        .values(["b1", "b2"].map(|id| Business { id: id.to_string(), accounts: Vec::new() }))
        .execute(client)
        .await?;
    insert_many::<Account>()
        .values(
            [("a1", "b1"), ("a2", "b1"), ("a3", "b2")]
                .map(|(id, business)| Account::new(id.to_string(), business.to_string(), format!("{id} name"))),
        )
        .execute(client)
        .await?;
    insert_many::<Transaction>()
        .values(TRANSACTIONS.map(|(id, account, kind, status, amount, fee, note, (day, hour, minute), settled)| {
            Transaction {
                id: id.to_string(),
                account_id: account.map(str::to_string),
                kind: kind.to_string(),
                status: status.to_string(),
                amount,
                fee,
                note: note.map(str::to_string),
                points: Points(amount / 10),
                created_at: at(day, hour, minute),
                settled_on: settled.map(|day| NaiveDate::from_ymd_opt(2026, 3, day).expect("valid date")),
                account: None,
            }
        }))
        .execute(client)
        .await?;
    insert_many::<Tag>()
        .values([("vip", 10), ("promo", 5), ("new", 1)].map(|(id, weight)| Tag {
            id: id.to_string(),
            name: id.to_string(),
            weight,
        }))
        .execute(client)
        .await?;
    let link = |account: &str, tag: &str| vec![DinocoValue::from(account), DinocoValue::from(tag)];
    client
        .backend
        .insert(InsertQuery {
            table: "_gi_account_to_gi_tag",
            fields: vec!["account_id", "tag_id"],
            rows: vec![link("a1", "vip"), link("a1", "promo"), link("a2", "vip")],
            returning: None,
            on_conflict_do_nothing: None,
        })
        .await?;
    Ok(())
}

fn accounts() -> dinoco::FindMany<Account> {
    find_many::<Account>().order_by(|x| x.id.asc())
}

/// Each account's groups as `(kind, amount)`.
fn kinds(accounts: &[Account]) -> Vec<(String, Vec<(String, i64)>)> {
    accounts
        .iter()
        .map(|account| {
            let groups = account.transactions.iter().map(|group| (group.kind.clone(), group.amount)).collect();
            (account.id.clone(), groups)
        })
        .collect()
}

fn groups(rows: &[(&str, &[(&str, i64)])]) -> Vec<(String, Vec<(String, i64)>)> {
    rows.iter()
        .map(|(id, groups)| (id.to_string(), groups.iter().map(|(kind, amount)| (kind.to_string(), *amount)).collect()))
        .collect()
}

async fn sums_grouped_by_key(client: DinocoClient) -> anyhow::Result<()> {
    let rows = accounts()
        .includes(|x| x.transactions().pluck(|t| t.amount.sum()).group_by(|t| t.kind))
        .execute(&client)
        .await?;
    assert_eq!(
        kinds(&rows),
        groups(&[("a1", &[("boleto", 30), ("card", 200), ("pix", 150)]), ("a2", &[("card", 150)]), ("a3", &[])])
    );

    // The parent is read as usual, and every other field of a group row keeps
    // its default.
    assert_eq!(rows[0].name, "a1 name");
    let group = &rows[0].transactions[0];
    let default = Transaction::default();
    assert_eq!((group.id.as_str(), group.status.as_str(), group.fee), ("", "", 0.0));
    assert_eq!((&group.account_id, &group.note, group.created_at), (&None, &None, default.created_at));
    Ok(())
}

async fn ungrouped_aggregates(client: DinocoClient) -> anyhow::Result<()> {
    let rows = accounts().includes(|x| x.transactions().pluck(|t| t.amount.sum())).execute(&client).await?;
    let totals = rows
        .iter()
        .map(|account| (account.id.as_str(), account.transactions.iter().map(|row| row.amount).collect::<Vec<_>>()))
        .collect::<Vec<_>>();
    // One row per parent with related rows; none for `a3`.
    assert_eq!(totals, [("a1", vec![380]), ("a2", vec![150]), ("a3", vec![])]);
    Ok(())
}

async fn groups_are_ordered_and_paged_per_parent(client: DinocoClient) -> anyhow::Result<()> {
    // The largest group of each account.
    let rows = accounts()
        .includes(|x| {
            x.transactions().order_by(|t| t.amount.desc()).take(1).pluck(|t| t.amount.sum()).group_by(|t| t.kind)
        })
        .execute(&client)
        .await?;
    assert_eq!(kinds(&rows), groups(&[("a1", &[("card", 200)]), ("a2", &[("card", 150)]), ("a3", &[])]));

    // The runner-up, when there is one.
    let rows = accounts()
        .includes(|x| {
            x.transactions()
                .order_by(|t| t.amount.desc())
                .skip(1)
                .take(1)
                .pluck(|t| t.amount.sum())
                .group_by(|t| t.kind)
        })
        .execute(&client)
        .await?;
    assert_eq!(kinds(&rows), groups(&[("a1", &[("pix", 150)]), ("a2", &[]), ("a3", &[])]));

    // By key, descending; `skip` alone.
    let rows = accounts()
        .includes(|x| {
            x.transactions().order_by(|t| t.kind.desc()).skip(1).pluck(|t| t.amount.sum()).group_by(|t| t.kind)
        })
        .execute(&client)
        .await?;
    assert_eq!(kinds(&rows), groups(&[("a1", &[("card", 200), ("boleto", 30)]), ("a2", &[]), ("a3", &[])]));

    // Ties on the aggregate are broken by the key, as the later `order_by` says.
    let rows = accounts()
        .includes(|x| {
            x.transactions()
                .order_by(|t| t.amount.asc())
                .order_by(|t| t.kind.desc())
                .pluck(|t| t.amount.count())
                .group_by(|t| t.kind)
        })
        .execute(&client)
        .await?;
    assert_eq!(
        kinds(&rows),
        groups(&[("a1", &[("card", 1), ("boleto", 1), ("pix", 2)]), ("a2", &[("card", 2)]), ("a3", &[])])
    );
    Ok(())
}

async fn filters_pick_the_rows(client: DinocoClient) -> anyhow::Result<()> {
    let rows = accounts()
        .includes(|x| x.transactions().where_(|t| t.status.eq("paid")).pluck(|t| t.amount.sum()).group_by(|t| t.kind))
        .execute(&client)
        .await?;
    assert_eq!(
        kinds(&rows),
        groups(&[("a1", &[("boleto", 30), ("card", 200), ("pix", 100)]), ("a2", &[("card", 150)]), ("a3", &[])])
    );

    let rows = accounts()
        .where_(|x| x.id.not_in_batch(["a3"]))
        .includes(|x| {
            x.transactions()
                .where_complex(|t, c| c.or(t.amount.gte(100_i64), t.note.eq("late")))
                .pluck(|t| t.amount.sum())
                .group_by(|t| t.kind)
        })
        .execute(&client)
        .await?;
    assert_eq!(
        kinds(&rows),
        groups(&[("a1", &[("boleto", 30), ("card", 200), ("pix", 100)]), ("a2", &[("card", 80)])])
    );

    // No related row matches: every account gets an empty list.
    let rows = accounts()
        .includes(|x| {
            x.transactions().where_(|t| t.amount.gt(10_000_i64)).pluck(|t| t.amount.sum()).group_by(|t| t.kind)
        })
        .execute(&client)
        .await?;
    assert!(rows.iter().all(|account| account.transactions.is_empty()));
    Ok(())
}

async fn every_aggregate_function(client: DinocoClient) -> anyhow::Result<()> {
    let a1 = |rows: Vec<Account>| rows.into_iter().next().expect("a1").transactions;

    // `count` fits the integer field it counts.
    let counts = a1(accounts()
        .includes(|x| x.transactions().pluck(|t| t.amount.count()).group_by(|t| t.kind))
        .execute(&client)
        .await?);
    assert_eq!(counts.iter().map(|row| row.amount).collect::<Vec<_>>(), [1, 1, 2]);

    // `avg` and a float `sum` fit a float field.
    let averages = a1(accounts()
        .includes(|x| x.transactions().pluck(|t| t.fee.avg()).group_by(|t| t.kind))
        .execute(&client)
        .await?);
    assert_eq!(averages.iter().map(|row| row.fee).collect::<Vec<_>>(), [0.0, 2.0, 0.75]);
    let fees = a1(accounts()
        .includes(|x| x.transactions().pluck(|t| t.fee.sum()).group_by(|t| t.kind))
        .execute(&client)
        .await?);
    assert_eq!(fees.iter().map(|row| row.fee).collect::<Vec<_>>(), [0.0, 2.0, 1.5]);

    // `min` and `max` fit the field they read.
    let smallest = a1(accounts()
        .includes(|x| x.transactions().pluck(|t| t.amount.min()).group_by(|t| t.kind))
        .execute(&client)
        .await?);
    assert_eq!(smallest.iter().map(|row| row.amount).collect::<Vec<_>>(), [30, 200, 50]);
    let latest = a1(accounts()
        .includes(|x| x.transactions().pluck(|t| t.created_at.max()).group_by(|t| t.kind))
        .execute(&client)
        .await?);
    assert_eq!(latest.iter().map(|row| row.created_at).collect::<Vec<_>>(), [at(3, 12, 0), at(2, 9, 0), at(1, 10, 45)]);
    let notes = a1(accounts()
        .includes(|x| x.transactions().pluck(|t| t.note.max()).group_by(|t| t.kind))
        .execute(&client)
        .await?);
    assert_eq!(
        notes.iter().map(|row| row.note.clone()).collect::<Vec<_>>(),
        [Some("late".into()), None, Some("retry".into())]
    );

    // Grouping and aggregating one field keeps the key.
    let same = a1(accounts()
        .includes(|x| x.transactions().pluck(|t| t.amount.count()).group_by(|t| t.amount))
        .execute(&client)
        .await?);
    assert_eq!(same.iter().map(|row| row.amount).collect::<Vec<_>>(), [30, 50, 100, 200]);
    Ok(())
}

async fn date_and_hour_buckets(client: DinocoClient) -> anyhow::Result<()> {
    let a1 = |rows: Vec<Account>| {
        rows.into_iter()
            .next()
            .expect("a1")
            .transactions
            .into_iter()
            .map(|row| (row.created_at, row.amount))
            .collect::<Vec<_>>()
    };

    // A day bucket of a timestamp is stored at midnight UTC.
    let days = a1(accounts()
        .includes(|x| x.transactions().pluck(|t| t.amount.sum()).group_by(|t| t.created_at.date()))
        .execute(&client)
        .await?);
    assert_eq!(days, [(midnight(1), 150), (midnight(2), 200), (midnight(3), 30)]);

    let hours = a1(accounts()
        .includes(|x| x.transactions().pluck(|t| t.amount.sum()).group_by(|t| t.created_at.hour()))
        .execute(&client)
        .await?);
    assert_eq!(hours, [(at(1, 10, 0), 150), (at(2, 9, 0), 200), (at(3, 12, 0), 30)]);

    // A date field's months, nullable: `NULL` dates are a group of their own.
    let rows = accounts()
        .includes(|x| x.transactions().pluck(|t| t.amount.sum()).group_by(|t| t.settled_on.month()))
        .execute(&client)
        .await?;
    // Where `NULL` sorts depends on the database (last on PostgreSQL).
    let mut months = rows[0].transactions.iter().map(|row| (row.settled_on, row.amount)).collect::<Vec<_>>();
    months.sort();
    let march = NaiveDate::from_ymd_opt(2026, 3, 1);
    assert_eq!(months, [(None, 50), (march, 330)]);
    Ok(())
}

async fn nullable_keys(client: DinocoClient) -> anyhow::Result<()> {
    let rows = accounts()
        .includes(|x| x.transactions().pluck(|t| t.amount.sum()).group_by(|t| t.note))
        .execute(&client)
        .await?;
    // Where the `NULL` group sorts depends on the database (last on PostgreSQL).
    let notes = rows
        .iter()
        .map(|account| {
            let mut notes = account.transactions.iter().map(|row| (row.note.clone(), row.amount)).collect::<Vec<_>>();
            notes.sort();
            notes
        })
        .collect::<Vec<_>>();
    assert_eq!(
        notes,
        [
            vec![(None, 300), (Some("late".to_string()), 30), (Some("retry".to_string()), 50)],
            vec![(None, 70), (Some("late".to_string()), 80)],
            vec![],
        ]
    );
    Ok(())
}

async fn many_to_many(client: DinocoClient) -> anyhow::Result<()> {
    let rows =
        accounts().includes(|x| x.tags().pluck(|t| t.weight.sum()).group_by(|t| t.name)).execute(&client).await?;
    let tags = rows
        .iter()
        .map(|account| account.tags.iter().map(|tag| (tag.name.as_str(), tag.weight)).collect::<Vec<_>>())
        .collect::<Vec<_>>();
    assert_eq!(tags, [vec![("promo", 5), ("vip", 10)], vec![("vip", 10)], vec![]]);

    let rows = accounts()
        .includes(|x| x.tags().where_(|t| t.weight.gt(1_i64)).pluck(|t| t.weight.sum()))
        .execute(&client)
        .await?;
    let totals =
        rows.iter().map(|account| account.tags.iter().map(|tag| tag.weight).collect::<Vec<_>>()).collect::<Vec<_>>();
    assert_eq!(totals, [vec![15], vec![10], vec![]]);

    let rows = accounts()
        .includes(|x| x.tags().order_by(|t| t.weight.desc()).take(1).pluck(|t| t.weight.sum()).group_by(|t| t.name))
        .execute(&client)
        .await?;
    assert_eq!(rows[0].tags.iter().map(|tag| tag.name.as_str()).collect::<Vec<_>>(), ["vip"]);
    Ok(())
}

async fn nested_includes(client: DinocoClient) -> anyhow::Result<()> {
    let businesses = find_many::<Business>()
        .order_by(|x| x.id.asc())
        .includes(|x| {
            x.accounts()
                .order_by(|a| a.id.asc())
                .includes(|a| a.transactions().pluck(|t| t.amount.sum()).group_by(|t| t.kind))
                .includes(|a| a.tags().order_by(|t| t.id.asc()))
        })
        .execute(&client)
        .await?;
    assert_eq!(
        kinds(&businesses[0].accounts),
        groups(&[("a1", &[("boleto", 30), ("card", 200), ("pix", 150)]), ("a2", &[("card", 150)])])
    );
    // Plain includes next to an aggregate one still load their rows.
    assert_eq!(businesses[0].accounts[0].tags.len(), 2);
    assert_eq!(kinds(&businesses[1].accounts), groups(&[("a3", &[])]));

    // From the other side: each transaction's account, with its totals.
    let transactions = find_many::<Transaction>()
        .where_(|x| x.id.batch(["t1", "t7"]))
        .order_by(|x| x.id.asc())
        .includes(|x| x.account().includes(|a| a.transactions().pluck(|t| t.amount.sum())))
        .execute(&client)
        .await?;
    let account = transactions[0].account.as_ref().expect("t1 account");
    assert_eq!(account.transactions.iter().map(|row| row.amount).collect::<Vec<_>>(), [380]);
    assert!(transactions[1].account.is_none());
    Ok(())
}

async fn runtime_structs(client: DinocoClient) -> anyhow::Result<()> {
    // A sum of points is points, stored in the points field.
    let rows = accounts()
        .includes(|x| x.transactions().pluck(|t| t.points.sum()).group_by(|t| t.kind))
        .execute(&client)
        .await?;
    let points = rows[0].transactions.iter().map(|row| (row.kind.as_str(), row.points)).collect::<Vec<_>>();
    assert_eq!(points, [("boleto", Points(3)), ("card", Points(20)), ("pix", Points(15))]);

    // And points are a key too.
    let rows = accounts()
        .includes(|x| x.transactions().pluck(|t| t.amount.count()).group_by(|t| t.points))
        .execute(&client)
        .await?;
    let keys = rows[1].transactions.iter().map(|row| (row.points, row.amount)).collect::<Vec<_>>();
    assert_eq!(keys, [(Points(7), 1), (Points(8), 1)]);
    Ok(())
}

async fn several_aggregates(client: DinocoClient) -> anyhow::Result<()> {
    let totals = |accounts: &[Account]| {
        accounts
            .iter()
            .map(|account| {
                account.transactions.iter().map(|row| (row.kind.clone(), row.amount, row.fee, row.points)).collect()
            })
            .collect::<Vec<Vec<_>>>()
    };
    let row = |kind: &str, amount: i64, fee: f64, points: i64| (kind.to_string(), amount, fee, Points(points));

    // Each aggregate is stored in the field it reads.
    let rows = accounts()
        .includes(|x| {
            x.transactions()
                .pluck(|t| t.amount.sum())
                .pluck(|t| t.fee.sum())
                .pluck(|t| t.points.sum())
                .group_by(|t| t.kind)
        })
        .execute(&client)
        .await?;
    assert_eq!(
        totals(&rows),
        [
            vec![row("boleto", 30, 0.0, 3), row("card", 200, 2.0, 20), row("pix", 150, 1.5, 15)],
            vec![row("card", 150, 1.0, 15)],
            vec![],
        ]
    );

    // Ungrouped: one row per parent.
    let rows = accounts()
        .includes(|x| x.transactions().pluck(|t| t.amount.sum()).pluck(|t| t.fee.avg()))
        .execute(&client)
        .await?;
    let pairs = rows
        .iter()
        .map(|account| account.transactions.iter().map(|row| (row.amount, row.fee)).collect::<Vec<_>>())
        .collect::<Vec<_>>();
    assert_eq!(pairs, [vec![(380, 0.875)], vec![(150, 0.5)], vec![]]);

    // `order_by` on the second aggregated field orders by its aggregate.
    let rows = accounts()
        .includes(|x| {
            x.transactions()
                .order_by(|t| t.fee.asc())
                .take(2)
                .pluck(|t| t.amount.sum())
                .pluck(|t| t.fee.sum())
                .group_by(|t| t.kind)
        })
        .execute(&client)
        .await?;
    assert_eq!(kinds(&rows), groups(&[("a1", &[("boleto", 30), ("pix", 150)]), ("a2", &[("card", 150)]), ("a3", &[])]));

    // Many-to-many too.
    let rows =
        accounts().includes(|x| x.tags().pluck(|t| t.weight.sum()).pluck(|t| t.name.max())).execute(&client).await?;
    let tags = rows
        .iter()
        .map(|account| account.tags.iter().map(|tag| (tag.weight, tag.name.clone())).collect::<Vec<_>>())
        .collect::<Vec<_>>();
    assert_eq!(tags, [vec![(15, "vip".to_string())], vec![(10, "vip".to_string())], vec![]]);
    Ok(())
}

async fn projections(client: DinocoClient) -> anyhow::Result<()> {
    // Each group is a projection of the related model, under a projection of
    // the parent.
    let rows = find_many::<Account>()
        .select::<AccountTotals>()
        .order_by(|x| x.id.asc())
        .includes(|x| {
            x.transactions()
                .select::<TransactionTotals>()
                .where_(|t| t.status.eq("paid"))
                .pluck(|t| t.amount.sum())
                .pluck(|t| t.fee.sum())
                .group_by(|t| t.kind)
        })
        .execute(&client)
        .await?;
    let totals = |kind: &str, amount: i64, fee: f64| TransactionTotals { kind: kind.to_string(), amount, fee };
    assert_eq!((rows[0].id.as_str(), rows[0].name.as_str()), ("a1", "a1 name"));
    assert_eq!(rows[0].transactions, [totals("boleto", 30, 0.0), totals("card", 200, 2.0), totals("pix", 100, 1.0)]);
    assert_eq!(rows[1].transactions, [totals("card", 150, 1.0)]);
    assert!(rows[2].transactions.is_empty());

    // One aggregate, ungrouped, into a projection.
    let rows = find_many::<Account>()
        .select::<AccountTotals>()
        .order_by(|x| x.id.asc())
        .includes(|x| x.transactions().select::<TransactionTotals>().pluck(|t| t.amount.sum()))
        .execute(&client)
        .await?;
    assert_eq!(rows[0].transactions, [totals("", 380, 0.0)]);

    // The parent alone can be a projection, with model rows for the groups.
    let rows = find_many::<Account>()
        .select::<AccountName>()
        .order_by(|x| x.id.asc())
        .includes(|x| x.transactions().pluck(|t| t.amount.sum()).group_by(|t| t.kind))
        .execute(&client)
        .await?;
    assert_eq!(
        rows[1].transactions.iter().map(|row| (row.kind.as_str(), row.amount)).collect::<Vec<_>>(),
        [("card", 150)]
    );

    // A projection of the parent with projection children inserts them as
    // the related model.
    dinoco::insert_into::<Account>()
        .values(NewAccount {
            id: "a9".to_string(),
            name: "a9 name".to_string(),
            transactions: vec![NewTransaction {
                id: "t9".to_string(),
                kind: "pix".to_string(),
                amount: 5,
                created_at: at(9, 12, 0),
            }],
        })
        .execute(&client)
        .await?;
    let inserted = find_many::<Transaction>().where_(|t| t.account_id.eq("a9")).execute(&client).await?;
    let inserted = inserted.iter().map(|row| (row.id.as_str(), row.kind.as_str(), row.amount)).collect::<Vec<_>>();
    assert_eq!(inserted, [("t9", "pix", 5)]);

    // A projection must hold every field filled.
    let error = find_many::<Account>()
        .select::<AccountKinds>()
        .includes(|x| x.transactions().select::<TransactionKind>().pluck(|t| t.amount.sum()).group_by(|t| t.kind))
        .execute(&client)
        .await
        .expect_err("no amount field");
    assert!(error.to_string().contains("`TransactionKind` has no field `amount`"), "{error}");
    Ok(())
}

async fn inside_transactions(client: DinocoClient) -> anyhow::Result<()> {
    let rows = transaction(&client, |tx| async move {
        dinoco::delete_many::<Transaction>().where_(|x| x.kind.eq("boleto")).execute(tx).await?;
        Ok(accounts().includes(|x| x.transactions().pluck(|t| t.amount.sum()).group_by(|t| t.kind)).execute(tx).await?)
    })
    .await?;
    // The include sees the transaction's own delete.
    assert_eq!(kinds(&rows), groups(&[("a1", &[("card", 200), ("pix", 150)]), ("a2", &[("card", 150)]), ("a3", &[])]));
    Ok(())
}

async fn errors(client: DinocoClient) -> anyhow::Result<()> {
    let message = |result: anyhow::Result<Vec<Account>>| result.expect_err("an invalid include").to_string();

    // An average is a float, and `amount` an integer.
    let error = message(
        accounts().includes(|x| x.transactions().pluck(|t| t.amount.avg()).group_by(|t| t.kind)).execute(&client).await,
    );
    assert!(error.contains("the avg of `amount`"), "{error}");
    assert!(error.contains("can't be stored in `gi_transaction.amount`"), "{error}");

    // A count is an integer, and `kind` a string.
    let error = message(
        accounts()
            .includes(|x| x.transactions().pluck(|t| t.kind.count()).group_by(|t| t.status))
            .execute(&client)
            .await,
    );
    assert!(error.contains("the count of `kind`"), "{error}");

    // Two aggregates of one field would be stored in the same field.
    let error = message(
        accounts()
            .includes(|x| x.transactions().pluck(|t| t.amount.sum()).pluck(|t| t.amount.max()).group_by(|t| t.kind))
            .execute(&client)
            .await,
    );
    assert!(error.contains("the sum of `amount` and the max of `amount` can't both be stored"), "{error}");

    // Every value of several aggregates must fit its field.
    let error = message(
        accounts()
            .includes(|x| x.transactions().pluck(|t| t.amount.sum()).pluck(|t| t.kind.count()).group_by(|t| t.status))
            .execute(&client)
            .await,
    );
    assert!(error.contains("the count of `kind` (a `i64`) can't be stored"), "{error}");

    // The groups have no column for other fields to order them by.
    let error = message(
        accounts()
            .includes(|x| x.transactions().order_by(|t| t.id.asc()).pluck(|t| t.amount.sum()).group_by(|t| t.kind))
            .execute(&client)
            .await,
    );
    assert!(error.contains("not by `id`"), "{error}");
    Ok(())
}

/// The statement each database gets for a paged, grouped include.
#[tokio::test]
async fn compiles_grouped_includes() -> anyhow::Result<()> {
    use dinoco_engine::{
        AggregateColumn, AggregateFunction, AggregateGroup, AggregateQuery, FindQuery, GroupBy, GroupOrder,
        RelationAggregateQuery,
    };

    let sqlite = SqliteAdapter::new(":memory:".to_string()).await.map_err(anyhow::Error::msg)?;
    let postgres = PostgresAdapter::pgbouncer(POSTGRES_URL).await?;
    let query = |limit: i32| RelationAggregateQuery {
        query: AggregateQuery {
            source: FindQuery::new(&[], "gi_transaction", -1, -1),
            aggregates: AggregateColumn { function: AggregateFunction::SumInteger, field: "amount" }.into(),
            group: Some(AggregateGroup {
                key: GroupBy::Field("kind"),
                order_by: Some(GroupOrder::AggregateDesc),
                then_by: None,
                ordered_aggregate: 0,
                limit,
                skip: -1,
            }),
        },
        child_field: "account_id",
        join: None,
        key_count: 2,
    };
    let keys = || vec![DinocoValue::from("a1"), DinocoValue::from("a2")];

    let (sql, params) = sqlite.compile_relation_aggregate_query_with_keys(query(-1), keys());
    assert_eq!(
        sql,
        "WITH __dinoco_parent_keys(__dinoco_relation_ordinal, __dinoco_relation_key) AS (VALUES (0, ?), (1, ?)) \
         SELECT kind AS __dinoco_group_key, COALESCE(SUM(amount), 0) AS __dinoco_aggregate, \
         __dinoco_parent_keys.__dinoco_relation_ordinal FROM __dinoco_parent_keys INNER JOIN gi_transaction ON \
         __dinoco_parent_keys.__dinoco_relation_key = gi_transaction.account_id GROUP BY \
         __dinoco_parent_keys.__dinoco_relation_ordinal, __dinoco_group_key ORDER BY \
         __dinoco_parent_keys.__dinoco_relation_ordinal, __dinoco_aggregate DESC, __dinoco_group_key ASC"
    );
    assert_eq!(params, keys());

    let (sql, params) = postgres.compile_relation_aggregate_query_with_keys(query(3), keys());
    assert!(sql.starts_with("SELECT * FROM (SELECT __dinoco_groups.*, ROW_NUMBER() OVER (PARTITION BY"), "{sql}");
    assert!(sql.contains("(VALUES (0, $1::text), (1, $2::text))"), "{sql}");
    assert!(
        sql.ends_with("WHERE __dinoco_row_num <= $3 ORDER BY __dinoco_relation_ordinal, __dinoco_row_num"),
        "{sql}"
    );
    assert_eq!(params.len(), 3);
    Ok(())
}

async fn run_on_sqlite<F, Fut>(name: &str, scenario: F) -> anyhow::Result<()>
where
    F: FnOnce(DinocoClient) -> Fut,
    Fut: Future<Output = anyhow::Result<()>>,
{
    let path = format!("/private/tmp/dinoco-gi-{name}-{}-{}.sqlite", std::process::id(), monotonic());
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
    create_table(adapter, "gi_business", vec![primary(string("id"))]).await?;
    create_table(adapter, "gi_account", vec![primary(string("id")), string("business_id"), string("name")]).await?;
    create_table(
        adapter,
        "gi_transaction",
        vec![
            primary(string("id")),
            nullable(string("account_id")),
            string("kind"),
            string("status"),
            integer("amount"),
            column("fee", MigrationColumnType::Float),
            nullable(string("note")),
            integer("points"),
            column("created_at", MigrationColumnType::DateTime),
            nullable(column("settled_on", MigrationColumnType::Date)),
        ],
    )
    .await?;
    create_table(adapter, "gi_tag", vec![primary(string("id")), string("name"), integer("weight")]).await?;
    create_table(adapter, "_gi_account_to_gi_tag", vec![string("account_id"), string("tag_id")]).await
}

fn monotonic() -> u128 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
}
