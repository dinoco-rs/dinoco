//! Relation filters: `where_(...)` on a relation field narrows the parent rows
//! by their related rows without loading them. Covers every relation shape
//! (many-to-one, one-to-many, both sides of a one-to-one, many-to-many, self
//! relations), the `where_`/`none`/`every`/`exists`/`not_exists` quantifiers
//! and their `_complex` forms,
//! nesting, `where_complex`, and every builder that accepts a `where_`.
//!
//! Every scenario runs on SQLite, PostgreSQL and MySQL (the same local servers
//! `docker_adapters.rs` uses).

use std::future::Future;

use dinoco::{
    DinocoEnum, Entity, FindWhere, WhereComplex, count, delete, delete_many, exists, find_and_update, find_first,
    find_many, insert_many, transaction, update, update_many,
};
use dinoco_engine::{
    Backend, CreateEnumMigration, DinocoAdapter, DinocoClient, DinocoSqlCompiler, DinocoValue, DropEnumMigration,
    InsertQuery, MigrationColumnType, MySqlAdapter, PostgresAdapter, SqliteAdapter,
};
use dinoco_tests::{column, create_table, drop_table, nullable, primary};

const POSTGRES_URL: &str = "postgres://postgres:postgres@localhost:5432/postgres";
const MYSQL_URL: &str = "mysql://root:root@localhost:3306/mysql";
static POSTGRES_TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
static MYSQL_TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

const TABLES: [&str; 8] = [
    "rf_bank",
    "rf_payout",
    "rf_transaction",
    "rf_item",
    "rf_receipt",
    "rf_tag",
    "_rf_tag_to_rf_transaction",
    "rf_employee",
];
const PAYOUT_METHODS: [&str; 3] = ["pix", "ted", "boleto"];

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
    many_to_one,
    one_to_many,
    one_to_one,
    many_to_many,
    nested,
    self_relation,
    where_complex,
    writes,
    includes_and_relation_counts,
    transactions,
    exists_complex_shapes,
    exists_complex_edges,
    exists_complex_reads,
    exists_complex_writes,
    exists_complex_transactions,
);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, DinocoEnum)]
pub enum PayoutMethod {
    #[default]
    #[dinoco(value = "pix")]
    Pix,
    #[dinoco(value = "ted")]
    Ted,
    #[dinoco(value = "boleto")]
    Boleto,
}

#[derive(Debug, Entity)]
#[dinoco(table_name = "rf_bank")]
pub struct Bank {
    #[dinoco(primary_key)]
    id: String,
    name: String,
}

#[derive(Debug, Entity)]
#[dinoco(table_name = "rf_payout")]
pub struct Payout {
    #[dinoco(primary_key)]
    id: String,
    method: PayoutMethod,
    bank_id: Option<String>,

    #[dinoco(many_to_one, foreign_key = "bank_id", references = "id")]
    bank: Option<Bank>,

    #[dinoco(one_to_many, foreign_key = "payout_id", references = "id")]
    transactions: Vec<Transaction>,
}

#[derive(Debug, Entity)]
#[dinoco(table_name = "rf_transaction")]
pub struct Transaction {
    #[dinoco(primary_key)]
    id: String,
    amount: i64,
    payout_id: Option<String>,

    #[dinoco(many_to_one, foreign_key = "payout_id", references = "id")]
    payout: Option<Payout>,

    #[dinoco(one_to_many, foreign_key = "transaction_id", references = "id")]
    items: Vec<Item>,

    #[dinoco(one_to_one, inverse, foreign_key = "transaction_id", references = "id")]
    receipt: Option<Box<Receipt>>,

    #[dinoco(
        many_to_many,
        foreign_key = "id",
        references = "id",
        join_table = "_rf_tag_to_rf_transaction",
        parent_field = "id",
        join_parent_field = "transaction_id",
        join_child_field = "tag_id"
    )]
    tags: Vec<Tag>,
}

#[derive(Debug, Entity)]
#[dinoco(table_name = "rf_item")]
pub struct Item {
    #[dinoco(primary_key)]
    id: String,
    transaction_id: String,
    sku: String,
    refunded: bool,
    note: Option<String>,

    #[dinoco(many_to_one, foreign_key = "transaction_id", references = "id")]
    transaction: Option<Transaction>,
}

#[derive(Debug, Entity)]
#[dinoco(table_name = "rf_receipt")]
pub struct Receipt {
    #[dinoco(primary_key)]
    id: String,
    transaction_id: String,
    number: String,

    #[dinoco(one_to_one, foreign_key = "transaction_id", references = "id")]
    transaction: Option<Box<Transaction>>,
}

#[derive(Debug, Entity)]
#[dinoco(table_name = "rf_tag")]
pub struct Tag {
    #[dinoco(primary_key)]
    id: String,
    name: String,

    #[dinoco(
        many_to_many,
        foreign_key = "id",
        references = "id",
        join_table = "_rf_tag_to_rf_transaction",
        parent_field = "id",
        join_parent_field = "tag_id",
        join_child_field = "transaction_id"
    )]
    transactions: Vec<Transaction>,
}

#[derive(Debug, Entity)]
#[dinoco(table_name = "rf_employee")]
pub struct Employee {
    #[dinoco(primary_key)]
    id: String,
    name: String,
    manager_id: Option<String>,

    #[dinoco(many_to_one, relation_name = "management", foreign_key = "manager_id", references = "id")]
    manager: Option<Box<Employee>>,

    #[dinoco(one_to_many, relation_name = "management", foreign_key = "manager_id", references = "id")]
    reports: Vec<Employee>,
}

async fn many_to_one(client: DinocoClient) -> anyhow::Result<()> {
    let transactions = find_many::<Transaction>()
        .where_(|transaction| transaction.payout.where_(|payout| payout.method.eq(PayoutMethod::Pix)))
        .order_by(|transaction| transaction.id.asc())
        .execute(&client)
        .await?;
    assert_eq!(ids(&transactions, |transaction| &transaction.id), ["t1", "t2"]);
    assert!(transactions.iter().all(|transaction| transaction.payout.is_none()), "a filter never loads the relation");

    let first = find_first::<Transaction>()
        .where_(|transaction| transaction.payout.where_(|payout| payout.method.eq(PayoutMethod::Pix)))
        .where_(|transaction| transaction.amount.gt(200))
        .execute(&client)
        .await?
        .expect("a pix transaction above 200");
    assert_eq!(first.id, "t2");

    assert_eq!(transaction_ids(&client, |t| t.payout.exists()).await?, ["t1", "t2", "t3", "t5"]);
    assert_eq!(transaction_ids(&client, |t| t.payout.not_exists()).await?, ["t4"]);
    assert_eq!(
        transaction_ids(&client, |t| t.payout.none(|payout| payout.method.eq(PayoutMethod::Pix))).await?,
        ["t3", "t4", "t5"],
        "a transaction without a payout has no pix payout"
    );
    assert_eq!(
        transaction_ids(&client, |t| t.payout.every(|payout| payout.method.eq(PayoutMethod::Ted))).await?,
        ["t3", "t4"],
        "every holds vacuously when there is no related row"
    );
    assert_eq!(
        transaction_ids(&client, |t| {
            t.payout.where_complex(|payout, m| {
                m.or(payout.method.eq(PayoutMethod::Ted), payout.method.eq(PayoutMethod::Boleto))
            })
        })
        .await?,
        ["t3", "t5"]
    );

    Ok(())
}

async fn one_to_many(client: DinocoClient) -> anyhow::Result<()> {
    assert_eq!(transaction_ids(&client, |t| t.items.where_(|item| item.refunded.eq(true))).await?, ["t2", "t5"]);
    assert_eq!(transaction_ids(&client, |t| t.items.none(|item| item.refunded.eq(true))).await?, ["t1", "t3", "t4"]);
    assert_eq!(
        transaction_ids(&client, |t| t.items.every(|item| item.refunded.eq(false))).await?,
        ["t1", "t3", "t4"],
        "t4 has no items, so every item matches vacuously"
    );
    assert_eq!(transaction_ids(&client, |t| t.items.every(|item| item.refunded.eq(true))).await?, ["t2", "t4"]);
    assert_eq!(transaction_ids(&client, |t| t.items.exists()).await?, ["t1", "t2", "t3", "t5"]);
    assert_eq!(transaction_ids(&client, |t| t.items.not_exists()).await?, ["t4"]);

    // `every` treats an item whose condition evaluates to NULL as not matching:
    // t1 and t5 each have an item without a note.
    assert_eq!(transaction_ids(&client, |t| t.items.every(|item| item.note.eq("gift"))).await?, ["t3", "t4"]);

    // Conditions inside one `where_` must hold on the same related row...
    assert_eq!(
        transaction_ids(&client, |t| {
            t.items.where_complex(|item, m| m.and([item.refunded.eq(true), item.note.eq("gift")]))
        })
        .await?,
        Vec::<String>::new()
    );
    // ...while separate relation filters may each be satisfied by a different row.
    let transactions = find_many::<Transaction>()
        .where_(|t| t.items.where_(|item| item.refunded.eq(true)))
        .where_(|t| t.items.where_(|item| item.note.eq("gift")))
        .execute(&client)
        .await?;
    assert_eq!(ids(&transactions, |transaction| &transaction.id), ["t5"]);

    // `none_complex` rejects a parent only when one item matches the whole
    // tree: t2's refunded item has a note, so t2 stays, unlike with
    // `none(|item| item.refunded.eq(true))`.
    assert_eq!(
        transaction_ids(&client, |t| t.items.none_complex(|item, m| m.and([item.refunded.eq(true), item.note.null()])))
            .await?,
        ["t1", "t2", "t3", "t4"]
    );
    // `every_complex` keeps the NULL rule of `every`: t1's item i2 is neither
    // refunded nor noted, so `false OR NULL` does not count as matching.
    assert_eq!(
        transaction_ids(&client, |t| {
            t.items.every_complex(|item, m| m.or(item.refunded.eq(true), item.note.eq("gift")))
        })
        .await?,
        ["t2", "t3", "t4", "t5"]
    );

    Ok(())
}

async fn one_to_one(client: DinocoClient) -> anyhow::Result<()> {
    assert_eq!(
        transaction_ids(&client, |t| t.receipt.where_(|receipt| receipt.number.starts_with("R-"))).await?,
        ["t1"]
    );
    assert_eq!(transaction_ids(&client, |t| t.receipt.exists()).await?, ["t1", "t3"]);
    assert_eq!(transaction_ids(&client, |t| t.receipt.not_exists()).await?, ["t2", "t4", "t5"]);

    let receipts = find_many::<Receipt>()
        .where_(|receipt| receipt.transaction.where_(|transaction| transaction.amount.gt(200)))
        .execute(&client)
        .await?;
    assert_eq!(ids(&receipts, |receipt| &receipt.id), ["r3"]);

    Ok(())
}

async fn many_to_many(client: DinocoClient) -> anyhow::Result<()> {
    assert_eq!(transaction_ids(&client, |t| t.tags.where_(|tag| tag.name.eq("vip"))).await?, ["t1", "t5"]);
    assert_eq!(transaction_ids(&client, |t| t.tags.none(|tag| tag.name.eq("vip"))).await?, ["t2", "t3", "t4"]);
    assert_eq!(transaction_ids(&client, |t| t.tags.every(|tag| tag.name.eq("promo"))).await?, ["t2", "t3", "t4"]);
    assert_eq!(transaction_ids(&client, |t| t.tags.exists()).await?, ["t1", "t2", "t5"]);

    let tags = find_many::<Tag>()
        .where_(|tag| tag.transactions.where_(|transaction| transaction.amount.gt(200)))
        .execute(&client)
        .await?;
    assert_eq!(ids(&tags, |tag| &tag.id), ["promo"]);

    let tags = find_many::<Tag>()
        .where_(|tag| {
            tag.transactions
                .where_(|transaction| transaction.payout.where_(|payout| payout.method.eq(PayoutMethod::Boleto)))
        })
        .execute(&client)
        .await?;
    assert_eq!(ids(&tags, |tag| &tag.id), ["vip"]);

    Ok(())
}

async fn nested(client: DinocoClient) -> anyhow::Result<()> {
    assert_eq!(
        transaction_ids(&client, |t| t.payout.where_(|payout| payout.bank.where_(|bank| bank.name.eq("Nubank"))))
            .await?,
        ["t1", "t2"]
    );

    let payouts = find_many::<Payout>()
        .where_(|payout| {
            payout.transactions.where_(|transaction| transaction.items.where_(|item| item.refunded.eq(true)))
        })
        .order_by(|payout| payout.id.asc())
        .execute(&client)
        .await?;
    assert_eq!(ids(&payouts, |payout| &payout.id), ["p-boleto", "p-pix"]);

    let items = find_many::<Item>()
        .where_(|item| {
            item.transaction.where_(|transaction| {
                transaction.payout.where_(|payout| payout.bank.where_(|bank| bank.name.eq("Itau")))
            })
        })
        .execute(&client)
        .await?;
    assert_eq!(ids(&items, |item| &item.id), ["i4"]);

    Ok(())
}

async fn self_relation(client: DinocoClient) -> anyhow::Result<()> {
    assert_eq!(employee_ids(&client, |e| e.manager.where_(|manager| manager.name.eq("Ana"))).await?, ["bia"]);
    assert_eq!(employee_ids(&client, |e| e.reports.exists()).await?, ["ana", "bia"]);
    assert_eq!(employee_ids(&client, |e| e.reports.not_exists()).await?, ["caio", "duda", "eva"]);
    assert_eq!(
        employee_ids(&client, |e| e.manager.where_(|manager| manager.manager.where_(|boss| boss.name.eq("Ana"))))
            .await?,
        ["caio", "duda"]
    );
    assert_eq!(employee_ids(&client, |e| e.reports.where_(|report| report.reports.exists())).await?, ["ana"]);
    assert_eq!(
        employee_ids(&client, |e| e.reports.every(|report| report.name.starts_with("C"))).await?,
        ["caio", "duda", "eva"]
    );

    // Writes whose filter reads the table being written.
    let renamed = update_many::<Employee>()
        .where_(|e| e.manager.where_(|manager| manager.name.eq("Bia")))
        .update(|e| e.name.set("Report"))
        .returning::<Employee>()
        .execute(&client)
        .await?;
    let mut renamed = ids(&renamed, |employee| &employee.id);
    renamed.sort();
    assert_eq!(renamed, ["caio", "duda"]);
    update::<Employee>()
        .where_(|e| e.reports.where_(|report| report.name.eq("Report")))
        .update(|e| e.name.set("Lead"))
        .execute(&client)
        .await?;
    assert_eq!(employee_ids(&client, |e| e.name.eq("Lead")).await?, ["bia"]);
    delete_many::<Employee>()
        .where_(|e| e.manager.not_exists())
        .where_(|e| e.reports.not_exists())
        .execute(&client)
        .await?;
    assert_eq!(employee_ids(&client, |e| e.id.not_null()).await?, ["ana", "bia", "caio", "duda"]);
    update_many::<Employee>()
        .where_(|e| e.reports.every(|report| report.name.eq("Report")))
        .where_(|e| e.reports.exists())
        .update(|e| e.name.set("Boss"))
        .execute(&client)
        .await?;
    assert_eq!(employee_ids(&client, |e| e.name.eq("Boss")).await?, ["bia"]);

    Ok(())
}

async fn where_complex(client: DinocoClient) -> anyhow::Result<()> {
    assert_eq!(
        transaction_ids_complex(&client, |t, m| m.not(t.payout.where_(|payout| payout.method.eq(PayoutMethod::Pix))))
            .await?,
        ["t3", "t4", "t5"],
        "NOT keeps rows without a related row"
    );
    assert_eq!(
        transaction_ids_complex(&client, |t, m| {
            m.or(t.payout.where_(|payout| payout.method.eq(PayoutMethod::Ted)), t.amount.lt(60))
        })
        .await?,
        ["t3", "t4"]
    );
    assert_eq!(
        transaction_ids_complex(&client, |t, m| {
            m.and([
                t.amount.gte(75),
                m.or_many([t.receipt.exists(), t.tags.where_(|tag| tag.name.eq("vip"))]),
                m.not(t.items.where_(|item| item.sku.eq("sku-4"))),
            ])
        })
        .await?,
        ["t1", "t5"]
    );

    Ok(())
}

async fn writes(client: DinocoClient) -> anyhow::Result<()> {
    let total = count::<Transaction>()
        .where_(|t| t.payout.where_(|payout| payout.method.eq(PayoutMethod::Pix)))
        .execute(&client)
        .await?
        .total;
    assert_eq!(total, 2);

    assert!(
        exists::<Transaction>()
            .where_(|t| t.receipt.where_(|receipt| receipt.number.eq("X-003")))
            .execute(&client)
            .await?
    );
    assert!(
        !exists::<Transaction>()
            .where_complex(|t, m| m.and([t.receipt.exists(), t.payout.not_exists()]))
            .execute(&client)
            .await?
    );

    let updated = update_many::<Transaction>()
        .where_(|t| t.payout.where_(|payout| payout.method.eq(PayoutMethod::Boleto)))
        .update(|t| t.amount.set(999))
        .returning::<Transaction>()
        .execute(&client)
        .await?;
    assert_eq!(ids(&updated, |transaction| &transaction.id), ["t5"]);
    assert_eq!(updated[0].amount, 999);

    update::<Transaction>()
        .where_(|t| t.items.every(|item| item.refunded.eq(true)))
        .where_(|t| t.items.exists())
        .update(|t| t.amount.decrement(50))
        .execute(&client)
        .await?;
    assert_eq!(amount_of(&client, "t2").await?, 200);

    let bumped = find_and_update::<Transaction>()
        .where_(|t| t.receipt.where_(|receipt| receipt.number.eq("R-001")))
        .update(|t| t.amount.increment(1))
        .execute(&client)
        .await?;
    assert_eq!((bumped.id.as_str(), bumped.amount), ("t1", 101));

    // The written table is only reached through a nested relation.
    update_many::<Transaction>()
        .where_(|t| t.payout.where_(|payout| payout.transactions.where_(|other| other.amount.gt(300))))
        .update(|t| t.amount.increment(5))
        .execute(&client)
        .await?;
    assert_eq!(amount_of(&client, "t3").await?, 405);

    delete_many::<Item>()
        .where_(|item| item.transaction.where_(|t| t.payout.where_(|payout| payout.method.eq(PayoutMethod::Ted))))
        .execute(&client)
        .await?;
    let remaining = find_many::<Item>().order_by(|item| item.id.asc()).execute(&client).await?;
    assert_eq!(ids(&remaining, |item| &item.id), ["i1", "i2", "i3", "i5", "i6"]);

    delete::<Transaction>().where_(|t| t.items.not_exists()).execute(&client).await?;
    assert_eq!(transaction_ids(&client, |t| t.amount.gt(0)).await?, ["t1", "t2", "t5"]);

    Ok(())
}

async fn includes_and_relation_counts(client: DinocoClient) -> anyhow::Result<()> {
    let payouts = find_many::<Payout>()
        .includes(|payout| {
            payout.transactions().where_(|t| t.items.where_(|item| item.refunded.eq(true))).order_by(|t| t.id.asc())
        })
        .order_by(|payout| payout.id.asc())
        .execute(&client)
        .await?;
    let loaded = payouts
        .iter()
        .map(|payout| (payout.id.as_str(), ids(&payout.transactions, |transaction| &transaction.id)))
        .collect::<Vec<_>>();
    assert_eq!(loaded, [("p-boleto", vec!["t5".to_string()]), ("p-pix", vec!["t2".to_string()]), ("p-ted", vec![])]);

    // `take` switches the include to its partitioned query; the filter still applies per parent.
    let payouts = find_many::<Payout>()
        .includes(|payout| payout.transactions().where_(|t| t.items.exists()).order_by(|t| t.amount.desc()).take(1))
        .order_by(|payout| payout.id.asc())
        .execute(&client)
        .await?;
    let loaded = payouts
        .iter()
        .map(|payout| (payout.id.as_str(), ids(&payout.transactions, |transaction| &transaction.id)))
        .collect::<Vec<_>>();
    assert_eq!(
        loaded,
        [("p-boleto", vec!["t5".to_string()]), ("p-pix", vec!["t2".to_string()]), ("p-ted", vec!["t3".to_string()])]
    );

    let transactions = find_many::<Transaction>()
        .where_(|t| t.payout.exists())
        .includes(|t| t.payout().where_(|payout| payout.bank.exists()))
        .order_by(|t| t.id.asc())
        .execute(&client)
        .await?;
    let loaded = transactions
        .iter()
        .map(|transaction| (transaction.id.as_str(), transaction.payout.as_ref().map(|payout| payout.id.as_str())))
        .collect::<Vec<_>>();
    assert_eq!(loaded, [("t1", Some("p-pix")), ("t2", Some("p-pix")), ("t3", Some("p-ted")), ("t5", None)]);

    let promo = find_first::<Tag>()
        .where_(|tag| tag.id.eq("promo"))
        .includes(|tag| {
            tag.transactions()
                .where_(|t| t.payout.where_(|payout| payout.method.eq(PayoutMethod::Pix)))
                .order_by(|t| t.id.asc())
        })
        .execute(&client)
        .await?
        .expect("promo tag");
    assert_eq!(ids(&promo.transactions, |transaction| &transaction.id), ["t1", "t2"]);

    let counted = count::<Payout>()
        .where_(|payout| payout.id.eq("p-pix"))
        .includes(|payout| payout.transactions().where_(|t| t.tags.where_(|tag| tag.name.eq("vip"))))
        .execute(&client)
        .await?;
    assert_eq!(counted.transactions, Some(1));

    let counted =
        count::<Tag>().includes(|tag| tag.transactions().where_(|t| t.receipt.exists())).execute(&client).await?;
    assert_eq!(counted.transactions, Some(2), "t1 is tagged twice and has a receipt");

    Ok(())
}

async fn transactions(client: DinocoClient) -> anyhow::Result<()> {
    let pix = transaction(&client, |tx| async move {
        update_many::<Transaction>()
            .where_(|t| t.tags.where_(|tag| tag.name.eq("vip")))
            .update(|t| t.amount.increment(1000))
            .execute(tx)
            .await?;

        find_many::<Transaction>()
            .where_(|t| t.payout.where_(|payout| payout.method.eq(PayoutMethod::Pix)))
            .where_(|t| t.amount.gt(1000))
            .execute(tx)
            .await
    })
    .await?;
    assert_eq!(ids(&pix, |transaction| &transaction.id), ["t1"]);
    assert_eq!(amount_of(&client, "t5").await?, 1075);

    let refunded = find_many::<Transaction>()
        .where_(|t| t.items.where_(|item| item.refunded.eq(true)))
        .order_by(|t| t.id.asc())
        .execute(&client)
        .await?;
    assert_eq!(ids(&refunded, |transaction| &transaction.id), ["t2", "t5"]);
    let payout =
        find_first::<Payout>().where_(|payout| payout.transactions.none(|t| t.items.exists())).execute(&client).await?;
    assert_eq!(payout.map(|payout| payout.id), None, "every payout has a transaction with items");

    Ok(())
}

#[test]
fn existence_complex_filters_match_their_quantifier_counterparts() {
    let tree = |item: ItemWhere, m: WhereComplex| m.and([item.refunded.eq(true), item.note.null()]);

    assert_eq!(
        format!("{:?}", TransactionWhere::default().items.exists_complex(tree)),
        format!("{:?}", TransactionWhere::default().items.where_complex(tree))
    );
    assert_eq!(
        format!("{:?}", TransactionWhere::default().items.not_exists_complex(tree)),
        format!("{:?}", TransactionWhere::default().items.none_complex(tree))
    );
}

async fn exists_complex_shapes(client: DinocoClient) -> anyhow::Result<()> {
    // Many-to-one.
    assert_eq!(
        transaction_ids(&client, |t| {
            t.payout.exists_complex(|payout, m| {
                m.or(payout.method.eq(PayoutMethod::Ted), payout.method.eq(PayoutMethod::Boleto))
            })
        })
        .await?,
        ["t3", "t5"]
    );
    assert_eq!(
        transaction_ids(&client, |t| {
            t.payout.not_exists_complex(|payout, m| {
                m.or(payout.method.eq(PayoutMethod::Ted), payout.method.eq(PayoutMethod::Boleto))
            })
        })
        .await?,
        ["t1", "t2", "t4"],
        "a transaction without a payout has no matching payout"
    );

    // One-to-many: the whole tree must hold on one item.
    assert_eq!(
        transaction_ids(&client, |t| {
            t.items.exists_complex(|item, m| m.and([item.refunded.eq(true), item.note.null()]))
        })
        .await?,
        ["t5"]
    );
    assert_eq!(
        transaction_ids(&client, |t| {
            t.items.not_exists_complex(|item, m| m.and([item.refunded.eq(true), item.note.null()]))
        })
        .await?,
        ["t1", "t2", "t3", "t4"]
    );

    // One-to-one, both sides.
    assert_eq!(
        transaction_ids(&client, |t| {
            t.receipt
                .exists_complex(|receipt, m| m.and([receipt.number.starts_with("X-"), receipt.number.ends_with("003")]))
        })
        .await?,
        ["t3"]
    );
    assert_eq!(
        transaction_ids(&client, |t| {
            t.receipt.not_exists_complex(|receipt, m| {
                m.and([receipt.number.starts_with("X-"), receipt.number.ends_with("003")])
            })
        })
        .await?,
        ["t1", "t2", "t4", "t5"]
    );
    let receipts = find_many::<Receipt>()
        .where_(|receipt| receipt.transaction.exists_complex(|t, m| m.and([t.amount.gt(200), t.payout.exists()])))
        .order_by(|receipt| receipt.id.asc())
        .execute(&client)
        .await?;
    assert_eq!(ids(&receipts, |receipt| &receipt.id), ["r3"]);
    let receipts = find_many::<Receipt>()
        .where_(|receipt| receipt.transaction.not_exists_complex(|t, m| m.and([t.amount.gt(200), t.payout.exists()])))
        .order_by(|receipt| receipt.id.asc())
        .execute(&client)
        .await?;
    assert_eq!(ids(&receipts, |receipt| &receipt.id), ["r1"]);

    // Many-to-many, both sides.
    assert_eq!(
        transaction_ids(&client, |t| t.tags.exists_complex(|tag, m| m.or(tag.name.eq("vip"), tag.name.eq("promo"))))
            .await?,
        ["t1", "t2", "t5"]
    );
    assert_eq!(
        transaction_ids(&client, |t| {
            t.tags.not_exists_complex(|tag, m| m.and([tag.name.eq("promo"), tag.id.eq("promo")]))
        })
        .await?,
        ["t3", "t4", "t5"]
    );
    let tags = find_many::<Tag>()
        .where_(|tag| tag.transactions.exists_complex(|t, m| m.and([t.amount.lt(100), t.payout.exists()])))
        .order_by(|tag| tag.id.asc())
        .execute(&client)
        .await?;
    assert_eq!(ids(&tags, |tag| &tag.id), ["vip"]);
    let tags = find_many::<Tag>()
        .where_(|tag| tag.transactions.not_exists_complex(|t, m| m.and([t.amount.lt(100), t.payout.exists()])))
        .order_by(|tag| tag.id.asc())
        .execute(&client)
        .await?;
    assert_eq!(ids(&tags, |tag| &tag.id), ["promo"]);

    // Self relation, both directions.
    assert_eq!(
        employee_ids(&client, |e| {
            e.manager.exists_complex(|manager, m| m.or(manager.name.eq("Ana"), manager.name.eq("Bia")))
        })
        .await?,
        ["bia", "caio", "duda"]
    );
    assert_eq!(
        employee_ids(&client, |e| {
            e.reports.not_exists_complex(|report, m| m.and([report.name.starts_with("C"), report.reports.not_exists()]))
        })
        .await?,
        ["ana", "caio", "duda", "eva"]
    );

    Ok(())
}

async fn exists_complex_edges(client: DinocoClient) -> anyhow::Result<()> {
    // An empty AND matches every related row, an empty OR none of them.
    assert_eq!(transaction_ids(&client, |t| t.items.exists_complex(|_, m| m.and([]))).await?, ["t1", "t2", "t3", "t5"]);
    assert_eq!(transaction_ids(&client, |t| t.items.not_exists_complex(|_, m| m.and([]))).await?, ["t4"]);
    assert_eq!(transaction_ids(&client, |t| t.items.exists_complex(|_, m| m.or_many([]))).await?, Vec::<String>::new());
    assert_eq!(
        transaction_ids(&client, |t| t.items.not_exists_complex(|_, m| m.or_many([]))).await?,
        ["t1", "t2", "t3", "t4", "t5"]
    );

    // A related row whose tree evaluates to NULL does not match: i2 and i5
    // have no note, so `NOT (note = 'gift')` is NULL for them, and only t2's
    // "damaged" item matches.
    assert_eq!(
        transaction_ids(&client, |t| t.items.exists_complex(|item, m| m.not(item.note.eq("gift")))).await?,
        ["t2"]
    );
    assert_eq!(
        transaction_ids(&client, |t| t.items.not_exists_complex(|item, m| m.not(item.note.eq("gift")))).await?,
        ["t1", "t3", "t4", "t5"]
    );

    // Nested trees, each one checked against its own related row.
    assert_eq!(
        payout_ids(&client, |payout| {
            payout.transactions.exists_complex(|t, m| {
                m.and([
                    t.amount.gt(200),
                    t.items.exists_complex(|item, m| m.or(item.refunded.eq(true), item.note.eq("damaged"))),
                ])
            })
        })
        .await?,
        ["p-pix"]
    );
    assert_eq!(
        payout_ids(&client, |payout| {
            payout.transactions.not_exists_complex(|t, m| {
                m.and([
                    t.amount.lt(300),
                    t.items.not_exists_complex(|item, m| m.and([item.refunded.eq(true), item.note.null()])),
                ])
            })
        })
        .await?,
        ["p-boleto", "p-ted"]
    );

    // Inside the builder's own where_complex tree.
    assert_eq!(
        transaction_ids_complex(&client, |t, m| {
            m.and([
                t.amount.lt(300),
                m.not(t.receipt.exists_complex(|receipt, m| m.and([receipt.number.starts_with("R-")]))),
            ])
        })
        .await?,
        ["t2", "t4", "t5"]
    );
    assert_eq!(
        transaction_ids_complex(&client, |t, m| {
            m.or(
                t.items.exists_complex(|item, m| m.and([item.refunded.eq(true), item.note.null()])),
                t.payout.not_exists_complex(|payout, m| {
                    m.or(payout.method.eq(PayoutMethod::Pix), payout.method.eq(PayoutMethod::Ted))
                }),
            )
        })
        .await?,
        ["t4", "t5"]
    );

    // Unlike the builder's where_complex, a relation tree keeps the plain
    // where_ calls around it.
    let transactions = find_many::<Transaction>()
        .where_(|t| t.items.not_exists_complex(|item, m| m.and([item.refunded.eq(true), item.note.null()])))
        .where_(|t| t.amount.gt(60))
        .order_by(|t| t.id.asc())
        .execute(&client)
        .await?;
    assert_eq!(ids(&transactions, |transaction| &transaction.id), ["t1", "t2", "t3"]);

    // Same rows as their quantifier counterparts.
    assert_eq!(
        transaction_ids(&client, |t| t.tags.exists_complex(|tag, m| m.or(tag.name.eq("vip"), tag.id.eq("promo"))))
            .await?,
        transaction_ids(&client, |t| t.tags.where_complex(|tag, m| m.or(tag.name.eq("vip"), tag.id.eq("promo"))))
            .await?
    );
    assert_eq!(
        transaction_ids(&client, |t| t.tags.not_exists_complex(|tag, m| m.or(tag.name.eq("vip"), tag.id.eq("promo"))))
            .await?,
        transaction_ids(&client, |t| t.tags.none_complex(|tag, m| m.or(tag.name.eq("vip"), tag.id.eq("promo"))))
            .await?
    );

    Ok(())
}

async fn exists_complex_reads(client: DinocoClient) -> anyhow::Result<()> {
    let refunded_or_damaged = |item: ItemWhere, m: WhereComplex| m.or(item.refunded.eq(true), item.note.eq("damaged"));
    let total =
        count::<Transaction>().where_(|t| t.items.exists_complex(refunded_or_damaged)).execute(&client).await?.total;
    assert_eq!(total, 2);
    let total = count::<Transaction>()
        .where_(|t| t.items.not_exists_complex(refunded_or_damaged))
        .execute(&client)
        .await?
        .total;
    assert_eq!(total, 3);

    assert!(
        exists::<Transaction>()
            .where_(|t| {
                t.receipt.not_exists_complex(|receipt, m| {
                    m.or(receipt.number.starts_with("R-"), receipt.number.starts_with("X-"))
                })
            })
            .execute(&client)
            .await?
    );
    assert!(
        !exists::<Transaction>()
            .where_(|t| t.tags.exists_complex(|tag, m| m.and([tag.name.eq("vip"), tag.name.eq("promo")])))
            .execute(&client)
            .await?,
        "no single tag is both vip and promo"
    );

    let first = find_first::<Transaction>()
        .where_(|t| t.tags.exists_complex(|tag, m| m.and([tag.name.eq("promo")])))
        .where_(|t| t.amount.gt(200))
        .execute(&client)
        .await?
        .expect("a promo transaction above 200");
    assert_eq!(first.id, "t2");

    let loaded = |payouts: &[Payout]| {
        payouts
            .iter()
            .map(|payout| (payout.id.clone(), ids(&payout.transactions, |transaction| &transaction.id)))
            .collect::<Vec<_>>()
    };
    let payouts = find_many::<Payout>()
        .includes(|payout| {
            payout
                .transactions()
                .where_(|t| t.items.exists_complex(|item, m| m.and([item.refunded.eq(true), item.note.null()])))
                .order_by(|t| t.id.asc())
        })
        .order_by(|payout| payout.id.asc())
        .execute(&client)
        .await?;
    assert_eq!(
        loaded(&payouts),
        [
            ("p-boleto".to_string(), vec!["t5".to_string()]),
            ("p-pix".to_string(), vec![]),
            ("p-ted".to_string(), vec![])
        ]
    );

    // `take` switches the include to its partitioned query.
    let payouts = find_many::<Payout>()
        .includes(|payout| {
            payout
                .transactions()
                .where_(|t| t.items.not_exists_complex(|item, m| m.and([item.refunded.eq(true), item.note.null()])))
                .order_by(|t| t.amount.desc())
                .take(1)
        })
        .order_by(|payout| payout.id.asc())
        .execute(&client)
        .await?;
    assert_eq!(
        loaded(&payouts),
        [
            ("p-boleto".to_string(), vec![]),
            ("p-pix".to_string(), vec!["t2".to_string()]),
            ("p-ted".to_string(), vec!["t3".to_string()])
        ]
    );

    let transactions = find_many::<Transaction>()
        .includes(|t| {
            t.payout().where_(|payout| {
                payout.bank.not_exists_complex(|bank, m| m.or(bank.name.eq("Nubank"), bank.name.eq("Itau")))
            })
        })
        .order_by(|t| t.id.asc())
        .execute(&client)
        .await?;
    let loaded = transactions
        .iter()
        .map(|transaction| (transaction.id.as_str(), transaction.payout.as_ref().map(|payout| payout.id.as_str())))
        .collect::<Vec<_>>();
    assert_eq!(loaded, [("t1", None), ("t2", None), ("t3", None), ("t4", None), ("t5", Some("p-boleto"))]);

    let counted = count::<Payout>()
        .where_(|payout| payout.id.eq("p-pix"))
        .includes(|payout| {
            payout
                .transactions()
                .where_(|t| t.tags.exists_complex(|tag, m| m.or(tag.name.eq("vip"), tag.name.eq("promo"))))
        })
        .execute(&client)
        .await?;
    assert_eq!(counted.transactions, Some(2));
    let counted = count::<Payout>()
        .where_(|payout| payout.id.eq("p-pix"))
        .includes(|payout| {
            payout.transactions().where_(|t| t.tags.not_exists_complex(|tag, m| m.and([tag.name.eq("vip")])))
        })
        .execute(&client)
        .await?;
    assert_eq!(counted.transactions, Some(1));

    Ok(())
}

async fn exists_complex_writes(client: DinocoClient) -> anyhow::Result<()> {
    let updated = update_many::<Transaction>()
        .where_(|t| {
            t.payout.exists_complex(|payout, m| m.and([payout.method.eq(PayoutMethod::Pix), payout.bank.exists()]))
        })
        .update(|t| t.amount.increment(1))
        .returning::<Transaction>()
        .execute(&client)
        .await?;
    let mut updated =
        updated.iter().map(|transaction| (transaction.id.as_str(), transaction.amount)).collect::<Vec<_>>();
    updated.sort();
    assert_eq!(updated, [("t1", 101), ("t2", 251)]);

    // t2's only item is refunded and has a note, so it is the one transaction
    // with items and no item that is unrefunded or missing a note.
    update::<Transaction>()
        .where_(|t| t.items.not_exists_complex(|item, m| m.or(item.refunded.eq(false), item.note.null())))
        .where_(|t| t.items.exists())
        .update(|t| t.amount.set(0))
        .execute(&client)
        .await?;
    assert_eq!(amount_of(&client, "t2").await?, 0);
    assert_eq!(amount_of(&client, "t1").await?, 101);

    let bumped = find_and_update::<Transaction>()
        .where_(|t| t.receipt.exists_complex(|receipt, m| m.and([receipt.number.eq("X-003")])))
        .update(|t| t.amount.increment(1))
        .execute(&client)
        .await?;
    assert_eq!((bumped.id.as_str(), bumped.amount), ("t3", 401));

    // Amounts are now t1 101, t2 0, t3 401, t4 50, t5 75: only t3 is above
    // 200 and only t4 has no payout, so the items of every other transaction go.
    delete_many::<Item>()
        .where_(|item| item.transaction.not_exists_complex(|t, m| m.or(t.amount.gt(200), t.payout.not_exists())))
        .execute(&client)
        .await?;
    let remaining = find_many::<Item>().order_by(|item| item.id.asc()).execute(&client).await?;
    assert_eq!(ids(&remaining, |item| &item.id), ["i4"]);

    delete::<Transaction>()
        .where_(|t| t.items.not_exists_complex(|item, m| m.or(item.refunded.eq(true), item.refunded.eq(false))))
        .execute(&client)
        .await?;
    assert_eq!(transaction_ids(&client, |t| t.id.not_null()).await?, ["t3"]);

    // Writes whose filter reads the table being written.
    update_many::<Employee>()
        .where_(|e| {
            e.reports.exists_complex(|report, m| m.and([report.name.starts_with("C"), report.reports.not_exists()]))
        })
        .update(|e| e.name.set("Lead"))
        .execute(&client)
        .await?;
    assert_eq!(employee_ids(&client, |e| e.name.eq("Lead")).await?, ["bia"]);
    delete_many::<Employee>()
        .where_(|e| e.manager.not_exists_complex(|manager, m| m.or(manager.name.eq("Ana"), manager.name.eq("Lead"))))
        .execute(&client)
        .await?;
    assert_eq!(employee_ids(&client, |e| e.id.not_null()).await?, ["bia", "caio", "duda"]);

    Ok(())
}

async fn exists_complex_transactions(client: DinocoClient) -> anyhow::Result<()> {
    let untagged = transaction(&client, |tx| async move {
        update_many::<Transaction>()
            .where_(|t| t.tags.exists_complex(|tag, m| m.and([tag.name.eq("vip")])))
            .update(|t| t.amount.increment(1000))
            .execute(tx)
            .await?;

        find_many::<Transaction>()
            .where_(|t| t.tags.not_exists_complex(|tag, m| m.or(tag.name.eq("vip"), tag.name.eq("promo"))))
            .order_by(|t| t.id.asc())
            .execute(tx)
            .await
    })
    .await?;
    assert_eq!(ids(&untagged, |transaction| &transaction.id), ["t3", "t4"]);
    assert_eq!(amount_of(&client, "t1").await?, 1100);
    assert_eq!(amount_of(&client, "t5").await?, 1075);

    Ok(())
}

fn ids<T>(rows: &[T], id: impl Fn(&T) -> &String) -> Vec<String> {
    rows.iter().map(|row| id(row).clone()).collect()
}

async fn transaction_ids(
    client: &DinocoClient,
    filter: impl FnOnce(TransactionWhere) -> FindWhere,
) -> anyhow::Result<Vec<String>> {
    let rows = find_many::<Transaction>().where_(filter).order_by(|t| t.id.asc()).execute(client).await?;
    Ok(ids(&rows, |transaction| &transaction.id))
}

async fn transaction_ids_complex(
    client: &DinocoClient,
    filter: impl FnOnce(TransactionWhere, WhereComplex) -> FindWhere,
) -> anyhow::Result<Vec<String>> {
    let rows = find_many::<Transaction>().where_complex(filter).order_by(|t| t.id.asc()).execute(client).await?;
    Ok(ids(&rows, |transaction| &transaction.id))
}

async fn employee_ids(
    client: &DinocoClient,
    filter: impl FnOnce(EmployeeWhere) -> FindWhere,
) -> anyhow::Result<Vec<String>> {
    let rows = find_many::<Employee>().where_(filter).order_by(|e| e.id.asc()).execute(client).await?;
    Ok(ids(&rows, |employee| &employee.id))
}

async fn payout_ids(
    client: &DinocoClient,
    filter: impl FnOnce(PayoutWhere) -> FindWhere,
) -> anyhow::Result<Vec<String>> {
    let rows = find_many::<Payout>().where_(filter).order_by(|payout| payout.id.asc()).execute(client).await?;
    Ok(ids(&rows, |payout| &payout.id))
}

async fn amount_of(client: &DinocoClient, id: &str) -> anyhow::Result<i64> {
    let transaction = find_first::<Transaction>().where_(|t| t.id.eq(id)).execute(client).await?;
    Ok(transaction.expect("transaction").amount)
}

async fn run_on_sqlite<F, Fut>(name: &str, scenario: F) -> anyhow::Result<()>
where
    F: FnOnce(DinocoClient) -> Fut,
    Fut: Future<Output = anyhow::Result<()>>,
{
    let path = format!("/private/tmp/dinoco-rf-{name}-{}-{}.sqlite", std::process::id(), monotonic());
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
        .compile_drop_enum_migration(DropEnumMigration { name: "PayoutMethod".to_string() })
        .into_iter()
        .chain(adapter.compile_create_enum_migration(CreateEnumMigration {
            name: "PayoutMethod".to_string(),
            values: PAYOUT_METHODS.map(str::to_string).to_vec(),
        }));
    for statement in enum_statements {
        adapter.execute(&statement, &[]).await?;
    }

    let string = |name: &str| column(name, MigrationColumnType::String);
    create_table(adapter, "rf_bank", vec![primary(string("id")), string("name")]).await?;
    create_table(
        adapter,
        "rf_payout",
        vec![
            primary(string("id")),
            column(
                "method",
                MigrationColumnType::Enum {
                    name: "PayoutMethod".to_string(),
                    values: PAYOUT_METHODS.map(str::to_string).to_vec(),
                },
            ),
            nullable(string("bank_id")),
        ],
    )
    .await?;
    create_table(
        adapter,
        "rf_transaction",
        vec![primary(string("id")), column("amount", MigrationColumnType::Integer), nullable(string("payout_id"))],
    )
    .await?;
    create_table(
        adapter,
        "rf_item",
        vec![
            primary(string("id")),
            string("transaction_id"),
            string("sku"),
            column("refunded", MigrationColumnType::Boolean),
            nullable(string("note")),
        ],
    )
    .await?;
    create_table(adapter, "rf_receipt", vec![primary(string("id")), string("transaction_id"), string("number")])
        .await?;
    create_table(adapter, "rf_tag", vec![primary(string("id")), string("name")]).await?;
    create_table(adapter, "_rf_tag_to_rf_transaction", vec![string("tag_id"), string("transaction_id")]).await?;
    create_table(adapter, "rf_employee", vec![primary(string("id")), string("name"), nullable(string("manager_id"))])
        .await
}

/// Seeds:
///
/// | transaction | amount | payout (method, bank) | items (refunded, note)        | receipt | tags       |
/// | ----------- | ------ | --------------------- | ----------------------------- | ------- | ---------- |
/// | t1          | 100    | p-pix (pix, Nubank)   | i1 (no, gift), i2 (no, NULL)  | R-001   | vip, promo |
/// | t2          | 250    | p-pix                 | i3 (yes, damaged)             |         | promo      |
/// | t3          | 400    | p-ted (ted, Itau)     | i4 (no, gift)                 | X-003   |            |
/// | t4          | 50     |                       |                               |         |            |
/// | t5          | 75     | p-boleto (boleto, -)  | i5 (yes, NULL), i6 (no, gift) |         | vip        |
///
/// Employees: ana -> bia -> {caio, duda}; eva has no manager and no reports.
async fn seed(client: &DinocoClient) -> anyhow::Result<()> {
    insert_many::<Bank>()
        .values(vec![
            Bank::new("nubank".to_string(), "Nubank".to_string()),
            Bank::new("itau".to_string(), "Itau".to_string()),
        ])
        .execute(client)
        .await?;

    let payout = |id: &str, method, bank_id: Option<&str>| {
        let mut payout = Payout::new(id.to_string(), method);
        payout.bank_id = bank_id.map(str::to_string);
        payout
    };
    insert_many::<Payout>()
        .values(vec![
            payout("p-pix", PayoutMethod::Pix, Some("nubank")),
            payout("p-ted", PayoutMethod::Ted, Some("itau")),
            payout("p-boleto", PayoutMethod::Boleto, None),
        ])
        .execute(client)
        .await?;

    let transaction = |id: &str, amount, payout_id: Option<&str>| {
        let mut transaction = Transaction::new(id.to_string(), amount);
        transaction.payout_id = payout_id.map(str::to_string);
        transaction
    };
    insert_many::<Transaction>()
        .values(vec![
            transaction("t1", 100, Some("p-pix")),
            transaction("t2", 250, Some("p-pix")),
            transaction("t3", 400, Some("p-ted")),
            transaction("t4", 50, None),
            transaction("t5", 75, Some("p-boleto")),
        ])
        .execute(client)
        .await?;

    let item = |id: &str, transaction_id: &str, refunded, note: Option<&str>| {
        let mut item = Item::new(id.to_string(), transaction_id.to_string(), format!("sku-{}", &id[1..]), refunded);
        item.note = note.map(str::to_string);
        item
    };
    insert_many::<Item>()
        .values(vec![
            item("i1", "t1", false, Some("gift")),
            item("i2", "t1", false, None),
            item("i3", "t2", true, Some("damaged")),
            item("i4", "t3", false, Some("gift")),
            item("i5", "t5", true, None),
            item("i6", "t5", false, Some("gift")),
        ])
        .execute(client)
        .await?;

    insert_many::<Receipt>()
        .values(vec![
            Receipt::new("r1".to_string(), "t1".to_string(), "R-001".to_string()),
            Receipt::new("r3".to_string(), "t3".to_string(), "X-003".to_string()),
        ])
        .execute(client)
        .await?;

    insert_many::<Tag>()
        .values(vec![
            Tag::new("vip".to_string(), "vip".to_string()),
            Tag::new("promo".to_string(), "promo".to_string()),
        ])
        .execute(client)
        .await?;
    let link = |tag_id: &str, transaction_id: &str| {
        vec![DinocoValue::String(tag_id.to_string()), DinocoValue::String(transaction_id.to_string())]
    };
    client
        .backend
        .insert(InsertQuery {
            table: "_rf_tag_to_rf_transaction",
            fields: vec!["tag_id", "transaction_id"],
            rows: vec![link("vip", "t1"), link("promo", "t1"), link("promo", "t2"), link("vip", "t5")],
            returning: None,
            on_conflict_do_nothing: None,
        })
        .await?;

    let employee = |id: &str, name: &str, manager_id: Option<&str>| {
        let mut employee = Employee::new(id.to_string(), name.to_string());
        employee.manager_id = manager_id.map(str::to_string);
        employee
    };
    insert_many::<Employee>()
        .values(vec![
            employee("ana", "Ana", None),
            employee("bia", "Bia", Some("ana")),
            employee("caio", "Caio", Some("bia")),
            employee("duda", "Duda", Some("bia")),
            employee("eva", "Eva", None),
        ])
        .execute(client)
        .await?;

    Ok(())
}

fn monotonic() -> u128 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
}
