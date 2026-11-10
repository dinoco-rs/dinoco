//! `not_in_batch(...)`: keeps the rows whose column holds none of the values
//! (`NOT IN`). Covers strings and integers, the empty list, `NULL` columns,
//! `where_complex`, relation filters, many-to-many keys, `count`/`exists`,
//! `count_transform(...).remove(...)`, `update_many`/`delete_many`,
//! transactions and the SQL each database gets.
//!
//! Every scenario runs on SQLite, PostgreSQL and MySQL (the same local servers
//! `docker_adapters.rs` uses).

use std::future::Future;

use dinoco::{
    DinocoValue, Entity, FindWhere, count, delete_many, exists, find_many, insert_many, transaction, update_many,
};
use dinoco_engine::{
    Backend, DinocoAdapter, DinocoClient, DinocoSqlCompiler, FindQuery, InsertQuery, MigrationColumnType, MySqlAdapter,
    PostgresAdapter, SqliteAdapter,
};
use dinoco_tests::{column, create_table, drop_table, nullable, primary};

const POSTGRES_URL: &str = "postgres://dinoco:dinoco@localhost:5432/dinoco";
const MYSQL_URL: &str = "mysql://dinoco:dinoco@localhost:3306/dinoco";
static POSTGRES_TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
static MYSQL_TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

const TABLES: [&str; 4] = ["nb_account", "nb_transaction", "nb_tag", "_nb_tag_to_nb_transaction"];

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
    excludes_values,
    null_columns,
    where_complex_and_relations,
    many_to_many_keys,
    counts_and_writes,
    transactions,
);

#[derive(Debug, Entity)]
#[dinoco(table_name = "nb_account")]
pub struct Account {
    #[dinoco(primary_key)]
    id: String,
    name: String,

    #[dinoco(one_to_many, foreign_key = "account_id", references = "id")]
    transactions: Vec<Transaction>,
}

#[derive(Debug, Entity)]
#[dinoco(table_name = "nb_transaction")]
pub struct Transaction {
    #[dinoco(primary_key)]
    id: String,
    account_id: Option<String>,
    amount: i64,
    status: String,
    note: Option<String>,

    #[dinoco(many_to_one, foreign_key = "account_id", references = "id")]
    account: Option<Box<Account>>,

    #[dinoco(
        many_to_many,
        foreign_key = "id",
        references = "id",
        join_table = "_nb_tag_to_nb_transaction",
        parent_field = "id",
        join_parent_field = "transaction_id",
        join_child_field = "tag_id"
    )]
    tags: Vec<Tag>,

    #[dinoco(
        many_to_many_key,
        join_table = "_nb_tag_to_nb_transaction",
        parent_field = "id",
        join_parent_field = "transaction_id",
        join_child_field = "tag_id"
    )]
    tag_id: Option<String>,
}

#[derive(Debug, Entity)]
#[dinoco(table_name = "nb_tag")]
pub struct Tag {
    #[dinoco(primary_key)]
    id: String,
    name: String,
}

/// `(id, account, amount, status, note)`, tagged vip: t1, t2, t5; promo: t2, t3.
const TRANSACTIONS: [(&str, Option<&str>, i64, &str, Option<&str>); 6] = [
    ("t1", Some("a1"), 100, "paid", Some("first")),
    ("t2", Some("a1"), 200, "pending", None),
    ("t3", Some("a2"), 50, "paid", Some("third")),
    ("t4", Some("a2"), 75, "refunded", None),
    ("t5", None, 10, "paid", Some("fifth")),
    ("t6", Some("a1"), 30, "failed", Some("sixth")),
];

async fn seed(client: &DinocoClient) -> anyhow::Result<()> {
    insert_many::<Account>()
        .values(["a1", "a2", "a3"].map(|id| Account::new(id.to_string(), format!("{id} name"))))
        .execute(client)
        .await?;
    insert_many::<Transaction>()
        .values(TRANSACTIONS.map(|(id, account, amount, status, note)| {
            let mut transaction = Transaction::new(id.to_string(), amount, status.to_string());
            transaction.account_id = account.map(str::to_string);
            transaction.note = note.map(str::to_string);
            transaction
        }))
        .execute(client)
        .await?;
    insert_many::<Tag>()
        .values(["vip", "promo"].map(|id| Tag::new(id.to_string(), id.to_string())))
        .execute(client)
        .await?;
    let link = |tag: &str, transaction: &str| vec![DinocoValue::from(tag), DinocoValue::from(transaction)];
    client
        .backend
        .insert(InsertQuery {
            table: "_nb_tag_to_nb_transaction",
            fields: vec!["tag_id", "transaction_id"],
            rows: vec![
                link("vip", "t1"),
                link("vip", "t2"),
                link("vip", "t5"),
                link("promo", "t2"),
                link("promo", "t3"),
            ],
            returning: None,
            on_conflict_do_nothing: None,
        })
        .await?;
    Ok(())
}

async fn ids<F>(client: &DinocoClient, filter: F) -> anyhow::Result<Vec<String>>
where
    F: FnOnce(TransactionWhere) -> FindWhere,
{
    find_many::<Transaction>().where_(filter).order_by(|x| x.id.asc()).pluck(|x| x.id).execute(client).await
}

async fn excludes_values(client: DinocoClient) -> anyhow::Result<()> {
    assert_eq!(ids(&client, |x| x.status.not_in_batch(["paid", "failed"])).await?, ["t2", "t4"]);
    assert_eq!(ids(&client, |x| x.amount.not_in_batch([100_i64, 200, 10])).await?, ["t3", "t4", "t6"]);
    // A single value, and values nothing holds.
    assert_eq!(ids(&client, |x| x.status.not_in_batch(["paid"])).await?, ["t2", "t4", "t6"]);
    assert_eq!(ids(&client, |x| x.status.not_in_batch(["unknown"])).await?.len(), 6);
    // An empty list excludes nothing (and `batch` of one keeps nothing).
    assert_eq!(ids(&client, |x| x.status.not_in_batch(Vec::<String>::new())).await?.len(), 6);
    assert!(ids(&client, |x| x.status.batch(Vec::<String>::new())).await?.is_empty());

    // Combined with `batch` on another column.
    let rows = find_many::<Transaction>()
        .where_(|x| x.account_id.batch(["a1", "a2"]))
        .where_(|x| x.status.not_in_batch(["paid"]))
        .order_by(|x| x.id.asc())
        .pluck(|x| x.id)
        .execute(&client)
        .await?;
    assert_eq!(rows, ["t2", "t4", "t6"]);
    Ok(())
}

async fn null_columns(client: DinocoClient) -> anyhow::Result<()> {
    // A `NULL` column is in no list, and in no `NOT IN` list either.
    assert_eq!(ids(&client, |x| x.note.not_in_batch(["first"])).await?, ["t3", "t5", "t6"]);
    assert_eq!(ids(&client, |x| x.account_id.not_in_batch(["a1"])).await?, ["t3", "t4"]);

    // Keep them explicitly with `null()`.
    let rows = find_many::<Transaction>()
        .where_complex(|x, c| c.or(x.account_id.not_in_batch(["a1"]), x.account_id.null()))
        .order_by(|x| x.id.asc())
        .pluck(|x| x.id)
        .execute(&client)
        .await?;
    assert_eq!(rows, ["t3", "t4", "t5"]);
    Ok(())
}

async fn where_complex_and_relations(client: DinocoClient) -> anyhow::Result<()> {
    let rows = find_many::<Transaction>()
        .where_complex(|x, c| c.and([c.not(x.amount.lt(50_i64)), x.status.not_in_batch(["pending", "refunded"])]))
        .order_by(|x| x.id.asc())
        .pluck(|x| x.id)
        .execute(&client)
        .await?;
    assert_eq!(rows, ["t1", "t3"]);

    // Accounts with a transaction that is neither paid nor pending.
    let accounts = find_many::<Account>()
        .where_(|x| x.transactions.where_(|t| t.status.not_in_batch(["paid", "pending"])))
        .order_by(|x| x.id.asc())
        .pluck(|x| x.id)
        .execute(&client)
        .await?;
    assert_eq!(accounts, ["a1", "a2"]);
    // Accounts whose transactions are all outside these statuses (a3 has none).
    let accounts = find_many::<Account>()
        .where_(|x| x.transactions.every(|t| t.status.not_in_batch(["refunded", "failed"])))
        .order_by(|x| x.id.asc())
        .pluck(|x| x.id)
        .execute(&client)
        .await?;
    assert_eq!(accounts, ["a3"]);

    // Transactions whose account is not `a1`.
    assert_eq!(ids(&client, |x| x.account.where_(|a| a.id.not_in_batch(["a1"]))).await?, ["t3", "t4"]);

    // Inside an include's filter.
    let accounts = find_many::<Account>()
        .order_by(|x| x.id.asc())
        .includes(|x| x.transactions().where_(|t| t.status.not_in_batch(["paid"])).order_by(|t| t.id.asc()))
        .execute(&client)
        .await?;
    let included = accounts.iter().map(|account| account.transactions.len()).collect::<Vec<_>>();
    assert_eq!(included, [2, 1, 0]);
    Ok(())
}

async fn many_to_many_keys(client: DinocoClient) -> anyhow::Result<()> {
    // Not linked to `vip`, untagged rows included: the same as `not_in`.
    assert_eq!(ids(&client, |x| x.tag_id.not_in_batch(["vip"])).await?, ["t3", "t4", "t6"]);
    assert_eq!(ids(&client, |x| x.tag_id.not_in(["vip"])).await?, ["t3", "t4", "t6"]);
    assert_eq!(ids(&client, |x| x.tag_id.not_in_batch(["vip", "promo"])).await?, ["t4", "t6"]);
    Ok(())
}

async fn counts_and_writes(client: DinocoClient) -> anyhow::Result<()> {
    let counted = count::<Transaction>().where_(|x| x.status.not_in_batch(["paid"])).execute(&client).await?;
    assert_eq!(counted.total, 3);
    assert!(exists::<Transaction>().where_(|x| x.status.not_in_batch(["paid", "pending"])).execute(&client).await?);
    assert!(
        !exists::<Transaction>()
            .where_(|x| x.status.not_in_batch(["paid", "pending", "refunded", "failed"]))
            .execute(&client)
            .await?
    );

    // `remove` drops a `not_in_batch` filter from a counting find's count.
    let (rows, count) = find_many::<Transaction>()
        .where_(|x| x.status.not_in_batch(["paid"]))
        .order_by(|x| x.id.asc())
        .count_transform(|count| count.remove(|x| x.status))
        .execute(&client)
        .await?;
    assert_eq!(rows.len(), 3);
    assert_eq!(count.total, 6);

    update_many::<Transaction>()
        .where_(|x| x.status.not_in_batch(["paid", "pending"]))
        .update(|x| x.status.set("closed"))
        .execute(&client)
        .await?;
    assert_eq!(ids(&client, |x| x.status.eq("closed")).await?, ["t4", "t6"]);

    delete_many::<Transaction>().where_(|x| x.id.not_in_batch(["t1", "t2"])).execute(&client).await?;
    assert_eq!(ids(&client, |x| x.amount.gt(0_i64)).await?, ["t1", "t2"]);
    Ok(())
}

async fn transactions(client: DinocoClient) -> anyhow::Result<()> {
    let left = transaction(&client, |tx| async move {
        delete_many::<Transaction>().where_(|x| x.status.not_in_batch(["paid"])).execute(tx).await?;
        Ok(find_many::<Transaction>()
            .where_(|x| x.id.not_in_batch(["t5"]))
            .order_by(|x| x.id.asc())
            .pluck(|x| x.id)
            .execute(tx)
            .await?)
    })
    .await?;
    assert_eq!(left, ["t1", "t3"]);
    Ok(())
}

/// The SQL is `NOT IN` with one placeholder per value, and an empty list is
/// always true.
#[tokio::test]
async fn compiles_to_not_in() -> anyhow::Result<()> {
    let sqlite = SqliteAdapter::new(":memory:".to_string()).await.map_err(anyhow::Error::msg)?;
    let postgres = PostgresAdapter::pgbouncer(POSTGRES_URL).await?;
    let mysql = MySqlAdapter::new(MYSQL_URL);
    let compilers: [(&dyn DinocoSqlCompiler, &str); 3] =
        [(&sqlite, "status NOT IN (?, ?)"), (&postgres, "status NOT IN ($1, $2)"), (&mysql, "status NOT IN (?, ?)")];
    for (compiler, expected) in compilers {
        let mut query = FindQuery::new(&["id"], "nb_transaction", -1, -1);
        query.conditions = vec![TransactionWhere::default().status.not_in_batch(["paid", "failed"])];
        let (sql, params) = compiler.compile_find_query(query);
        assert!(sql.contains(expected), "{sql}");
        assert_eq!(params, [DinocoValue::from("paid"), DinocoValue::from("failed")]);

        let mut query = FindQuery::new(&["id"], "nb_transaction", -1, -1);
        query.conditions = vec![TransactionWhere::default().status.not_in_batch(Vec::<String>::new())];
        let (sql, params) = compiler.compile_find_query(query);
        assert!(sql.ends_with("WHERE 1 = 1"), "{sql}");
        assert!(params.is_empty());
    }
    Ok(())
}

async fn run_on_sqlite<F, Fut>(name: &str, scenario: F) -> anyhow::Result<()>
where
    F: FnOnce(DinocoClient) -> Fut,
    Fut: Future<Output = anyhow::Result<()>>,
{
    let path = format!("/private/tmp/dinoco-nb-{name}-{}-{}.sqlite", std::process::id(), monotonic());
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
    create_table(adapter, "nb_account", vec![primary(string("id")), string("name")]).await?;
    create_table(
        adapter,
        "nb_transaction",
        vec![
            primary(string("id")),
            nullable(string("account_id")),
            column("amount", MigrationColumnType::Integer),
            string("status"),
            nullable(string("note")),
        ],
    )
    .await?;
    create_table(adapter, "nb_tag", vec![primary(string("id")), string("name")]).await?;
    create_table(adapter, "_nb_tag_to_nb_transaction", vec![string("tag_id"), string("transaction_id")]).await
}

fn monotonic() -> u128 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
}
