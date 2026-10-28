//! `find_many(...).count()` and its variants return the rows together with a
//! count of the same model: the find's own filters (`count`), those filters
//! changed for the count only (`count_transform`, with `remove`), or filters
//! of its own (`count_custom`, `count_custom_complex`). Covers paging (the
//! count ignores `take`, `skip` and `order_by`, and a page past the last row
//! runs no find), `where_complex`, relation filters, many-to-many keys,
//! relation counts, `select`/`includes`/`transform`, the last count call
//! winning, transactions, hooks, and error paths.
//!
//! Every scenario runs on SQLite, PostgreSQL and MySQL (the same local servers
//! `docker_adapters.rs` uses).

use std::future::Future;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use dinoco::{
    Entity, EntityExtend, TransactionContext, find_many, insert_into, insert_many, remove_test_methods,
    setup_test_methods, transaction,
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

const TABLES: [&str; 6] =
    ["fc_account", "fc_transaction", "fc_item", "fc_tag", "_fc_tag_to_fc_transaction", "fc_missing"];

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
    counts_the_find_filters,
    pages,
    transformed_filters,
    removes_by_relation_and_key,
    custom_filters,
    last_count_wins,
    rows_keep_their_stages,
    inside_transactions,
    errors,
);

#[derive(Debug, Entity)]
#[dinoco(table_name = "fc_account")]
pub struct Account {
    #[dinoco(primary_key)]
    id: String,
    name: String,

    #[dinoco(one_to_many, foreign_key = "account_id", references = "id")]
    transactions: Vec<Transaction>,
}

#[derive(Debug, Entity)]
#[dinoco(table_name = "fc_transaction")]
pub struct Transaction {
    #[dinoco(primary_key)]
    id: String,
    account_id: Option<String>,
    amount: i64,
    status: String,
    note: Option<String>,

    #[dinoco(many_to_one, foreign_key = "account_id", references = "id")]
    account: Option<Account>,

    #[dinoco(one_to_many, foreign_key = "transaction_id", references = "id")]
    items: Vec<Item>,

    #[dinoco(
        many_to_many,
        foreign_key = "id",
        references = "id",
        join_table = "_fc_tag_to_fc_transaction",
        parent_field = "id",
        join_parent_field = "transaction_id",
        join_child_field = "tag_id"
    )]
    tags: Vec<Tag>,

    #[dinoco(
        many_to_many_key,
        join_table = "_fc_tag_to_fc_transaction",
        parent_field = "id",
        join_parent_field = "transaction_id",
        join_child_field = "tag_id"
    )]
    tag_id: Option<String>,
}

#[derive(Debug, Entity)]
#[dinoco(table_name = "fc_item")]
pub struct Item {
    #[dinoco(primary_key)]
    id: String,
    transaction_id: String,
    refunded: bool,

    #[dinoco(many_to_one, foreign_key = "transaction_id", references = "id")]
    transaction: Option<Transaction>,
}

#[derive(Debug, Entity)]
#[dinoco(table_name = "fc_tag")]
pub struct Tag {
    #[dinoco(primary_key)]
    id: String,
    name: String,

    #[dinoco(
        many_to_many,
        foreign_key = "id",
        references = "id",
        join_table = "_fc_tag_to_fc_transaction",
        parent_field = "id",
        join_parent_field = "tag_id",
        join_child_field = "transaction_id"
    )]
    transactions: Vec<Transaction>,
}

#[derive(Debug, EntityExtend)]
#[extend(Transaction)]
pub struct TransactionAmount {
    pub id: String,
    pub amount: i64,
}

/// A table that is never created, to make the count fail.
#[derive(Debug, Entity)]
#[dinoco(table_name = "fc_missing")]
pub struct Missing {
    #[dinoco(primary_key)]
    id: String,
}

/// `fc_transaction` with a column it doesn't have: the count (`COUNT(*)`)
/// succeeds and the find fails.
#[derive(Debug, Entity)]
#[dinoco(table_name = "fc_transaction")]
pub struct Ghost {
    #[dinoco(primary_key)]
    id: String,
    ghost: String,
}

async fn counts_the_find_filters(client: DinocoClient) -> anyhow::Result<()> {
    // The count ignores `order_by`, `take` and `skip`; the rows keep them.
    let (rows, count): (Vec<Transaction>, TransactionCount) = find_many::<Transaction>()
        .where_(|t| t.items.not_empty())
        .order_by(|t| t.amount.desc())
        .skip(1)
        .take(2)
        .count()
        .execute(&client)
        .await?;
    assert_eq!(ids(&rows), ["t2", "t7"]);
    assert_eq!(count.total, 6);
    assert_eq!((count.items, count.tags), (None, None), "no relation count was asked for");

    // `where_complex`, and the `where_` it discards.
    let (rows, count) = find_many::<Transaction>()
        .where_complex(|t, m| m.or(t.status.eq("pending"), t.amount.gte(300)))
        .where_(|t| t.id.eq("t3"))
        .order_by(|t| t.id.asc())
        .take(1)
        .count()
        .execute(&client)
        .await?;
    assert_eq!((ids(&rows), count.total), (vec!["t3".to_string()], 3));

    // No filter: every row.
    let (rows, count) = find_many::<Transaction>().take(3).count().execute(&client).await?;
    assert_eq!((rows.len(), count.total), (3, 8));

    // Nothing matches.
    let (rows, count) = find_many::<Transaction>().where_(|t| t.status.eq("nope")).count().execute(&client).await?;
    assert!(rows.is_empty());
    assert_eq!(count.total, 0);

    // The filters are read when the query runs, so a `where_` after `count()` counts too.
    let (rows, count) =
        find_many::<Transaction>().count().where_(|t| t.status.eq("paid")).take(1).execute(&client).await?;
    assert_eq!((rows.len(), count.total), (1, 5));

    // A NULL check and a relation filter on the many-to-one side.
    let (rows, count) = find_many::<Transaction>()
        .where_(|t| t.account_id.null())
        .where_(|t| t.items.where_(|item| item.refunded.eq(true)))
        .count()
        .execute(&client)
        .await?;
    assert_eq!((ids(&rows), count.total), (vec!["t8".to_string()], 1));

    Ok(())
}

async fn pages(client: DinocoClient) -> anyhow::Result<()> {
    let finds = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&finds);
    setup_test_methods::<Transaction>(&client).on_find(move |_, _, _| {
        counter.fetch_add(1, Ordering::SeqCst);
    });

    let page = |index: i32| {
        let client = &client;
        async move {
            let (rows, count) = find_many::<Transaction>()
                .order_by(|t| t.id.asc())
                .skip(index * 3)
                .take(3)
                .includes(|t| t.items())
                .count()
                .execute(client)
                .await?;
            anyhow::Ok((ids(&rows), count.total))
        }
    };
    assert_eq!(page(0).await?, (strings(&["t1", "t2", "t3"]), 8));
    assert_eq!(page(1).await?, (strings(&["t4", "t5", "t6"]), 8));
    assert_eq!(page(2).await?, (strings(&["t7", "t8"]), 8));
    assert_eq!(finds.load(Ordering::SeqCst), 3);

    // Past the last row the count already says the page is empty: no find runs.
    assert_eq!(page(3).await?, (Vec::new(), 8));
    let (rows, count) = find_many::<Transaction>().skip(8).count().execute(&client).await?;
    assert_eq!((rows.len(), count.total), (0, 8));
    let (rows, count) = find_many::<Transaction>().where_(|t| t.status.eq("nope")).count().execute(&client).await?;
    assert_eq!((rows.len(), count.total), (0, 0));
    assert_eq!(finds.load(Ordering::SeqCst), 3, "no find past the last row");

    // A count with other filters says nothing about the find's rows, so the find runs.
    let (rows, count) = find_many::<Transaction>()
        .order_by(|t| t.id.asc())
        .skip(6)
        .count_custom(|count| count.where_(|t| t.status.eq("pending")))
        .execute(&client)
        .await?;
    assert_eq!((ids(&rows), count.total), (strings(&["t7", "t8"]), 2));
    let (rows, count) = find_many::<Transaction>()
        .where_(|t| t.status.eq("paid"))
        .skip(5)
        .count_transform(|count| count.remove(|t| t.status))
        .execute(&client)
        .await?;
    assert_eq!((rows.len(), count.total), (0, 8));
    assert_eq!(finds.load(Ordering::SeqCst), 5);

    remove_test_methods::<Transaction>(&client);
    Ok(())
}

async fn transformed_filters(client: DinocoClient) -> anyhow::Result<()> {
    // Plain `where_` calls: each one is removed or kept on its own.
    let paid_over_100 = || {
        find_many::<Transaction>().where_(|t| t.status.eq("paid")).where_(|t| t.amount.gt(100)).order_by(|t| t.id.asc())
    };
    let (rows, count) = paid_over_100().count_transform(|count| count.remove(|t| t.amount)).execute(&client).await?;
    assert_eq!(ids(&rows), ["t2", "t7", "t8"], "the find keeps its own filters");
    assert_eq!(count.total, 5);
    let (_, count) = paid_over_100()
        .count_transform(|count| count.remove(|t| t.status).where_(|t| t.status.eq("pending")))
        .execute(&client)
        .await?;
    assert_eq!(count.total, 2, "t3 and t6");
    let (_, count) = paid_over_100()
        .count_transform(|count| count.remove(|t| t.status).remove(|t| t.amount))
        .execute(&client)
        .await?;
    assert_eq!(count.total, 8);
    let (_, count) = paid_over_100().count_transform(|count| count.remove(|t| t.note)).execute(&client).await?;
    assert_eq!(count.total, 3, "nothing filters on note");
    let (_, count) = paid_over_100().count_transform(|count| count).execute(&client).await?;
    assert_eq!(count.total, 3, "unchanged, the find's filters");

    // A `where_complex` tree: members of an `and` group go one by one, an `or`
    // or `not` group that mentions the field goes as a whole.
    let tree = || {
        find_many::<Transaction>().where_complex(|t, m| {
            m.and([t.status.eq("paid"), t.amount.gte(120), m.or(t.note.eq("gift"), t.account_id.null())])
        })
    };
    let (rows, count) = tree().order_by(|t| t.id.asc()).count().execute(&client).await?;
    assert_eq!((ids(&rows), count.total), (strings(&["t7", "t8"]), 2));
    let (_, count) = tree().count_transform(|count| count.remove(|t| t.amount)).execute(&client).await?;
    assert_eq!(count.total, 4, "paid, and gift or without an account: t1, t4, t7, t8");
    let (_, count) = tree().count_transform(|count| count.remove(|t| t.note)).execute(&client).await?;
    assert_eq!(count.total, 3, "the whole `or` goes: paid from 120, t2, t7, t8");
    let (_, count) = tree().count_transform(|count| count.remove(|t| t.account_id)).execute(&client).await?;
    assert_eq!(count.total, 3);

    // Nested `and` groups, emptied groups, and `not`.
    let (_, count) = find_many::<Transaction>()
        .where_complex(|t, m| m.and([t.status.eq("paid"), m.and([t.amount.gte(120), t.note.null()])]))
        .count_transform(|count| count.remove(|t| t.amount))
        .execute(&client)
        .await?;
    assert_eq!(count.total, 3, "paid without a note: t2, t7, t8");
    let (_, count) = find_many::<Transaction>()
        .where_complex(|t, m| m.and([t.status.eq("paid"), m.and([t.amount.gte(120)])]))
        .count_transform(|count| count.remove(|t| t.amount))
        .execute(&client)
        .await?;
    assert_eq!(count.total, 5);
    let (_, count) = find_many::<Transaction>()
        .where_complex(|t, m| m.and([t.status.eq("paid"), m.not(t.amount.lt(120))]))
        .count_transform(|count| count.remove(|t| t.amount))
        .execute(&client)
        .await?;
    assert_eq!(count.total, 5);
    let (_, count) = find_many::<Transaction>()
        .where_complex(|t, m| m.or(t.status.eq("pending"), t.amount.lt(60)))
        .count_transform(|count| count.remove(|t| t.amount))
        .execute(&client)
        .await?;
    assert_eq!(count.total, 8, "a top-level `or` that mentions the field goes whole");

    // `where_complex` in the transform replaces the base, and discards later `where_` calls.
    let (_, count) = find_many::<Transaction>()
        .where_(|t| t.status.eq("paid"))
        .count_transform(|count| {
            count.where_complex(|t, m| m.or(t.amount.lt(60), t.amount.gt(450))).where_(|t| t.id.eq("t4"))
        })
        .execute(&client)
        .await?;
    assert_eq!(count.total, 2, "t4 and t8");

    // Values the transform captures.
    let floor = 300_i64;
    let (_, count) = find_many::<Transaction>()
        .where_(|t| t.amount.gte(100))
        .count_transform(move |count| count.remove(|t| t.amount).where_(|t| t.amount.gte(floor)))
        .execute(&client)
        .await?;
    assert_eq!(count.total, 3, "t3, t6, t8");

    Ok(())
}

async fn removes_by_relation_and_key(client: DinocoClient) -> anyhow::Result<()> {
    // Relation filters go by relation field; scalar fields of the related
    // model never match the parent's.
    let tagged_paid = || find_many::<Transaction>().where_(|t| t.tags.not_empty()).where_(|t| t.status.eq("paid"));
    let (rows, count) = tagged_paid().order_by(|t| t.id.asc()).count().execute(&client).await?;
    assert_eq!((ids(&rows), count.total), (strings(&["t1", "t2", "t8"]), 3));
    let (_, count) = tagged_paid().count_transform(|count| count.remove(|t| t.tags)).execute(&client).await?;
    assert_eq!(count.total, 5);
    let (_, count) = tagged_paid().count_transform(|count| count.remove(|t| t.items)).execute(&client).await?;
    assert_eq!(count.total, 3, "no filter on items");
    let (_, count) = find_many::<Transaction>()
        .where_(|t| t.items.where_(|item| item.id.eq("i3")))
        .count_transform(|count| count.remove(|t| t.id))
        .execute(&client)
        .await?;
    assert_eq!(count.total, 1, "the item's id is not the transaction's");
    let (_, count) = find_many::<Transaction>()
        .where_(|t| t.account.where_(|account| account.name.eq("Ana")))
        .where_(|t| t.amount.gt(200))
        .count_transform(|count| count.remove(|t| t.account))
        .execute(&client)
        .await?;
    assert_eq!(count.total, 4, "over 200: t2, t3, t6, t8");

    // A many-to-many key is removed by itself, not by its relation.
    let vip = || find_many::<Transaction>().where_(|t| t.tag_id.eq("vip"));
    let (rows, count) = vip().order_by(|t| t.id.asc()).count().execute(&client).await?;
    assert_eq!((ids(&rows), count.total), (strings(&["t1", "t2", "t5"]), 3));
    let (_, count) = vip().count_transform(|count| count.remove(|t| t.tag_id)).execute(&client).await?;
    assert_eq!(count.total, 8);
    let (_, count) = vip().count_transform(|count| count.remove(|t| t.tags)).execute(&client).await?;
    assert_eq!(count.total, 3);

    Ok(())
}

async fn custom_filters(client: DinocoClient) -> anyhow::Result<()> {
    let paid = || find_many::<Transaction>().where_(|t| t.status.eq("paid")).order_by(|t| t.id.asc()).take(2);

    let (rows, count) = paid().count_custom(|count| count.where_(|t| t.status.eq("pending"))).execute(&client).await?;
    assert_eq!((ids(&rows), count.total), (strings(&["t1", "t2"]), 2));
    let (_, count) = paid()
        .count_custom(|count| count.where_(|t| t.status.eq("pending")).where_(|t| t.amount.gt(350)))
        .execute(&client)
        .await?;
    assert_eq!(count.total, 1);
    let (_, count) = paid().count_custom(|count| count).execute(&client).await?;
    assert_eq!(count.total, 8, "an empty custom count counts every row");

    let (rows, count) =
        paid().count_custom_complex(|t, m| m.or(t.status.eq("refunded"), t.note.eq("gift"))).execute(&client).await?;
    assert_eq!((ids(&rows), count.total), (strings(&["t1", "t2"]), 4));
    let (_, count) = paid()
        .count_custom(|count| count.where_complex(|t, m| m.and([t.amount.lt(100)])).where_(|t| t.id.eq("t4")))
        .execute(&client)
        .await?;
    assert_eq!(count.total, 2, "where_complex discards the later where_: t4 and t5");

    // Relation counts, like `count::<M>().includes(...)`.
    let (_, count) = paid()
        .count_custom(|count| {
            count
                .where_(|t| t.account_id.eq("a1"))
                .includes(|t| t.items())
                .includes(|t| t.tags().where_(|tag| tag.name.eq("vip")))
        })
        .execute(&client)
        .await?;
    assert_eq!((count.total, count.items, count.tags), (3, Some(3), Some(2)));
    let (_, count) = paid()
        .count_transform(|count| count.includes(|t| t.items().where_(|item| item.refunded.eq(true))))
        .execute(&client)
        .await?;
    assert_eq!((count.total, count.items, count.tags), (5, Some(2), None), "refunded items of paid: i3, i8");

    Ok(())
}

async fn last_count_wins(client: DinocoClient) -> anyhow::Result<()> {
    let paid = || find_many::<Transaction>().where_(|t| t.status.eq("paid"));

    let (_, count) =
        paid().count_custom(|count| count.where_(|t| t.status.eq("pending"))).count().execute(&client).await?;
    assert_eq!(count.total, 5);
    let (_, count) = paid()
        .count()
        .count_custom_complex(|t, m| m.or(t.status.eq("pending"), t.status.eq("refunded")))
        .execute(&client)
        .await?;
    assert_eq!(count.total, 3);
    let (_, count) = paid()
        .count_custom(|count| count.where_(|t| t.status.eq("pending")))
        .count_custom_complex(|t, m| m.and([t.amount.lt(100)]))
        .execute(&client)
        .await?;
    assert_eq!(count.total, 2);
    let (_, count) = paid()
        .count_transform(|count| count.remove(|t| t.status))
        .count_custom(|count| count.where_(|t| t.status.eq("refunded")))
        .execute(&client)
        .await?;
    assert_eq!(count.total, 1);

    Ok(())
}

async fn rows_keep_their_stages(client: DinocoClient) -> anyhow::Result<()> {
    let (rows, count) = find_many::<Transaction>()
        .order_by(|t| t.id.asc())
        .take(2)
        .includes(|t| t.items().order_by(|item| item.id.asc()))
        .includes(|t| t.account())
        .count()
        .execute(&client)
        .await?;
    let loaded = rows
        .iter()
        .map(|t| (t.id.as_str(), item_ids(t), t.account.as_ref().map(|account| account.name.as_str())))
        .collect::<Vec<_>>();
    assert_eq!(loaded, [("t1", strings(&["i1", "i2"]), Some("Ana")), ("t2", strings(&["i3"]), Some("Ana"))]);
    assert_eq!(count.total, 8);

    let (amounts, count) = find_many::<Transaction>()
        .where_(|t| t.amount.gte(300))
        .order_by(|t| t.amount.asc())
        .select::<TransactionAmount>()
        .count()
        .execute(&client)
        .await?;
    let amounts = amounts.iter().map(|row| (row.id.as_str(), row.amount)).collect::<Vec<_>>();
    assert_eq!(amounts, [("t6", 300), ("t3", 400), ("t8", 500)]);
    assert_eq!(count.total, 3);

    let (ids, count): (Vec<String>, TransactionCount) = find_many::<Transaction>()
        .order_by(|t| t.amount.desc())
        .take(2)
        .count_custom(|count| count.where_(|t| t.items.empty()))
        .transform(|t| t.id)
        .execute(&client)
        .await?;
    assert_eq!((ids, count.total), (strings(&["t8", "t3"]), 2));

    let (rows, count) =
        find_many::<Transaction>().where_(|t| t.tags.empty()).read_in_primary().count().execute(&client).await?;
    assert_eq!((rows.len(), count.total), (3, 3), "t4, t6, t7");

    Ok(())
}

async fn inside_transactions(client: DinocoClient) -> anyhow::Result<()> {
    let (rows, count) = transaction(&client, |tx| async move {
        let mut extra = Transaction::new("t9".to_string(), 600, "pending".to_string());
        extra.account_id = Some("a3".to_string());
        insert_into::<Transaction>().values(extra).execute(tx).await?;
        insert_into::<Item>().values(Item::new("i9".to_string(), "t9".to_string(), false)).execute(tx).await?;

        let (rows, _) = find_many::<Transaction>()
            .where_(|t| t.status.eq("pending"))
            .order_by(|t| t.amount.desc())
            .take(1)
            .count()
            .execute(tx)
            .await?;
        assert_eq!(ids(&rows), ["t9"]);

        find_many::<Transaction>()
            .where_(|t| t.status.eq("pending"))
            .order_by(|t| t.id.asc())
            .count_transform(|count| count.remove(|t| t.status).includes(|t| t.items()))
            .execute(tx)
            .await
    })
    .await?;
    assert_eq!((ids(&rows), count.total, count.items), (strings(&["t3", "t6", "t9"]), 9, Some(9)));

    let (_, count) = find_many::<Transaction>().count().execute(&client).await?;
    assert_eq!(count.total, 9, "committed");

    Ok(())
}

async fn errors(client: DinocoClient) -> anyhow::Result<()> {
    let missing = find_many::<Missing>().count().execute(&client).await;
    assert!(missing.is_err(), "the count fails on a missing table");

    // The count succeeds, then the find fails.
    let ghost = find_many::<Ghost>().count().execute(&client).await;
    assert!(ghost.is_err(), "the find fails on a missing column");

    let in_transaction = transaction(&client, |tx| async move {
        insert_into::<Transaction>()
            .values(Transaction::new("t9".to_string(), 1, "paid".to_string()))
            .execute(tx)
            .await?;
        find_many::<Ghost>().count().execute(tx).await?;
        Ok(())
    })
    .await;
    assert!(in_transaction.is_err());
    let (_, count) = find_many::<Transaction>().count().execute(&client).await?;
    assert_eq!(count.total, 8, "the failed find rolled the transaction back");

    Ok(())
}

#[allow(dead_code)]
fn futures_are_send(client: &DinocoClient) {
    fn assert_send<T: Send>(_: T) {}

    assert_send(find_many::<Transaction>().count().execute(client));
    assert_send(find_many::<Transaction>().count().execute(TransactionContext));
    assert_send(find_many::<Transaction>().count_transform(|count| count.includes(|t| t.items())).execute(client));
    assert_send(find_many::<Transaction>().count_custom(|count| count).transform(|t| t.amount).execute(client));
    assert_send(find_many::<Transaction>().count_custom_complex(|t, m| m.and([t.amount.gt(1)])).execute(client));
}

fn ids(rows: &[Transaction]) -> Vec<String> {
    rows.iter().map(|row| row.id.clone()).collect()
}

fn item_ids(transaction: &Transaction) -> Vec<String> {
    transaction.items.iter().map(|item| item.id.clone()).collect()
}

fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| value.to_string()).collect()
}

async fn run_on_sqlite<F, Fut>(name: &str, scenario: F) -> anyhow::Result<()>
where
    F: FnOnce(DinocoClient) -> Fut,
    Fut: Future<Output = anyhow::Result<()>>,
{
    let path = format!("/private/tmp/dinoco-fc-{name}-{}-{}.sqlite", std::process::id(), monotonic());
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

/// Drops every table, `fc_missing` included, and creates all but that one.
async fn reset_schema<A>(adapter: &A) -> anyhow::Result<()>
where
    A: DinocoAdapter + DinocoSqlCompiler,
{
    for table in TABLES.iter().rev() {
        drop_table(adapter, table).await?;
    }

    let string = |name: &str| column(name, MigrationColumnType::String);
    create_table(adapter, "fc_account", vec![primary(string("id")), string("name")]).await?;
    create_table(
        adapter,
        "fc_transaction",
        vec![
            primary(string("id")),
            nullable(string("account_id")),
            column("amount", MigrationColumnType::Integer),
            string("status"),
            nullable(string("note")),
        ],
    )
    .await?;
    create_table(
        adapter,
        "fc_item",
        vec![primary(string("id")), string("transaction_id"), column("refunded", MigrationColumnType::Boolean)],
    )
    .await?;
    create_table(adapter, "fc_tag", vec![primary(string("id")), string("name")]).await?;
    create_table(adapter, "_fc_tag_to_fc_transaction", vec![string("tag_id"), string("transaction_id")]).await
}

/// Seeds:
///
/// | transaction | account | amount | status   | note | items (refunded)   | tags       |
/// | ----------- | ------- | ------ | -------- | ---- | ------------------ | ---------- |
/// | t1          | a1 Ana  | 100    | paid     | gift | i1 (no), i2 (no)   | vip        |
/// | t2          | a1      | 250    | paid     |      | i3 (yes)           | vip, promo |
/// | t3          | a1      | 400    | pending  |      |                    | promo      |
/// | t4          | a2 Bia  | 50     | paid     | gift | i4 (no)            |            |
/// | t5          | a2      | 75     | refunded |      | i5 (yes)           | vip        |
/// | t6          | a2      | 300    | pending  | gift |                    |            |
/// | t7          |         | 125    | paid     |      | i6 (no)            |            |
/// | t8          |         | 500    | paid     |      | i7 (no), i8 (yes)  | promo      |
///
/// a3 Caio has no transactions.
async fn seed(client: &DinocoClient) -> anyhow::Result<()> {
    insert_many::<Account>()
        .values(vec![
            Account::new("a1".to_string(), "Ana".to_string()),
            Account::new("a2".to_string(), "Bia".to_string()),
            Account::new("a3".to_string(), "Caio".to_string()),
        ])
        .execute(client)
        .await?;

    let transaction = |id: &str, account_id: Option<&str>, amount, status: &str, note: Option<&str>| {
        let mut transaction = Transaction::new(id.to_string(), amount, status.to_string());
        transaction.account_id = account_id.map(str::to_string);
        transaction.note = note.map(str::to_string);
        transaction
    };
    insert_many::<Transaction>()
        .values(vec![
            transaction("t1", Some("a1"), 100, "paid", Some("gift")),
            transaction("t2", Some("a1"), 250, "paid", None),
            transaction("t3", Some("a1"), 400, "pending", None),
            transaction("t4", Some("a2"), 50, "paid", Some("gift")),
            transaction("t5", Some("a2"), 75, "refunded", None),
            transaction("t6", Some("a2"), 300, "pending", Some("gift")),
            transaction("t7", None, 125, "paid", None),
            transaction("t8", None, 500, "paid", None),
        ])
        .execute(client)
        .await?;

    let item =
        |id: &str, transaction_id: &str, refunded| Item::new(id.to_string(), transaction_id.to_string(), refunded);
    insert_many::<Item>()
        .values(vec![
            item("i1", "t1", false),
            item("i2", "t1", false),
            item("i3", "t2", true),
            item("i4", "t4", false),
            item("i5", "t5", true),
            item("i6", "t7", false),
            item("i7", "t8", false),
            item("i8", "t8", true),
        ])
        .execute(client)
        .await?;

    insert_many::<Tag>()
        .values(["vip", "promo"].map(|name| Tag::new(name.to_string(), name.to_string())))
        .execute(client)
        .await?;
    let link = |tag_id: &str, transaction_id: &str| {
        vec![DinocoValue::String(tag_id.to_string()), DinocoValue::String(transaction_id.to_string())]
    };
    client
        .backend
        .insert(InsertQuery {
            table: "_fc_tag_to_fc_transaction",
            fields: vec!["tag_id", "transaction_id"],
            rows: vec![
                link("vip", "t1"),
                link("vip", "t2"),
                link("promo", "t2"),
                link("promo", "t3"),
                link("vip", "t5"),
                link("promo", "t8"),
            ],
            returning: None,
            on_conflict_do_nothing: None,
        })
        .await?;

    Ok(())
}

fn monotonic() -> u128 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
}
