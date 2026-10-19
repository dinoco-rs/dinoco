//! `count` and `exists` run against `&client` or against the `tx` of a
//! transaction closure. Through `tx` they read on the transaction's own
//! connection, so they see its uncommitted writes, including relation counts
//! from `.includes(...)`. Covers every relation shape (one-to-many,
//! many-to-one, both sides of a one-to-one, many-to-many, self relations),
//! nesting, `where_complex`, empty and `NULL` filters, every write builder,
//! hooks, and error paths.
//!
//! Every scenario runs on SQLite, PostgreSQL and MySQL (the same local servers
//! `docker_adapters.rs` uses).

use std::future::Future;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use dinoco::{
    Entity, MutationExecutor, ReadExecutor, RelationCount, TransactionContext, TransactionError, count, delete,
    delete_many, exists, find_and_update, find_many, insert_into, insert_many, remove_test_methods, setup_test_methods,
    transaction, transaction_with_error, update, update_many,
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

const TABLES: [&str; 7] =
    ["ce_store", "ce_profile", "ce_product", "ce_label", "_ce_label_to_product", "ce_category", "ce_missing"];

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
    reads_match_the_client,
    relation_counts_match_the_client,
    sees_uncommitted_writes,
    every_write_builder,
    concurrent_reads,
    errors_roll_back,
    reports_no_hooks,
);

#[derive(Debug, Entity)]
#[dinoco(table_name = "ce_store")]
pub struct Store {
    #[dinoco(primary_key)]
    id: String,
    name: String,
    city: Option<String>,

    #[dinoco(one_to_many, foreign_key = "store_id", references = "id")]
    products: Vec<Product>,

    #[dinoco(one_to_one, inverse, foreign_key = "store_id", references = "id")]
    profile: Option<Box<Profile>>,
}

#[derive(Debug, Entity)]
#[dinoco(table_name = "ce_profile")]
pub struct Profile {
    #[dinoco(primary_key)]
    id: String,
    store_id: String,
    verified: bool,

    #[dinoco(one_to_one, foreign_key = "store_id", references = "id")]
    store: Option<Box<Store>>,
}

#[derive(Debug, Entity)]
#[dinoco(table_name = "ce_product")]
pub struct Product {
    #[dinoco(primary_key)]
    id: String,
    price: i64,
    archived: bool,
    store_id: Option<String>,

    #[dinoco(many_to_one, foreign_key = "store_id", references = "id")]
    store: Option<Store>,

    #[dinoco(
        many_to_many,
        foreign_key = "id",
        references = "id",
        join_table = "_ce_label_to_product",
        parent_field = "id",
        join_parent_field = "product_id",
        join_child_field = "label_id"
    )]
    labels: Vec<Label>,
}

#[derive(Debug, Entity)]
#[dinoco(table_name = "ce_label")]
pub struct Label {
    #[dinoco(primary_key)]
    id: String,
    name: String,

    #[dinoco(
        many_to_many,
        foreign_key = "id",
        references = "id",
        join_table = "_ce_label_to_product",
        parent_field = "id",
        join_parent_field = "label_id",
        join_child_field = "product_id"
    )]
    products: Vec<Product>,
}

#[derive(Debug, Entity)]
#[dinoco(table_name = "ce_category")]
pub struct Category {
    #[dinoco(primary_key)]
    id: String,
    parent_id: Option<String>,

    #[dinoco(many_to_one, relation_name = "tree", foreign_key = "parent_id", references = "id")]
    parent: Option<Box<Category>>,

    #[dinoco(one_to_many, relation_name = "tree", foreign_key = "parent_id", references = "id")]
    children: Vec<Category>,
}

/// A table that is never created, to make a read fail.
#[derive(Debug, Entity)]
#[dinoco(table_name = "ce_missing")]
pub struct Missing {
    #[dinoco(primary_key)]
    id: String,
}

/// Every count and existence check of [`readings`], in order, with what the
/// seed gives.
const READINGS: [(&str, i64); 26] = [
    ("stores", 3),
    ("stores without a city", 1),
    ("active products from 100", 2),
    ("cheap or archived products (where_complex)", 2),
    ("where_complex discards where_", 3),
    ("empty batch", 0),
    ("stores with an archived product (one-to-many)", 1),
    ("stores whose products are all active (every)", 2),
    ("stores without an active product under 200 (none_complex)", 2),
    ("products in Recife (many-to-one)", 2),
    ("products without a store (not_exists)", 1),
    ("stores with a verified profile (one-to-one inverse)", 1),
    ("profiles of stores without a city (one-to-one)", 1),
    ("products on sale (many-to-many)", 2),
    ("labels without products (many-to-many)", 1),
    ("categories with children (self)", 2),
    ("children of top-level categories (self, nested)", 2),
    ("labels of verified stores (nested)", 2),
    ("exists: a store named Gamma", 1),
    ("exists: a store named Nope", 0),
    ("exists: an archived product on sale", 0),
    ("exists: cheap or in Natal (where_complex)", 1),
    ("exists: where_complex discards where_", 1),
    ("exists: empty batch", 0),
    ("exists: a product without a store id (NULL)", 1),
    ("exists: read_in_primary", 1),
];

/// Every relation count of [`relation_counts`], in order, with what the seed
/// gives: the label, `total`, and the relation's count.
const RELATION_COUNTS: [(&str, i64, Option<i64>); 12] = [
    ("products of every store (one-to-many)", 3, Some(3)),
    ("active products of Recife stores", 1, Some(1)),
    ("products of Gamma (no related rows)", 1, Some(0)),
    ("products of no store (no parents)", 0, Some(0)),
    ("products with labels (relation filter in the child)", 3, Some(2)),
    ("labels of every product (many-to-many)", 4, Some(3)),
    ("products over 200 on sale (many-to-many inverse)", 1, Some(1)),
    ("products of the empty label (no related rows)", 1, Some(0)),
    ("children of root (self)", 1, Some(2)),
    ("children of every category (self)", 5, Some(3)),
    ("children of a or orphan (where_complex parent)", 2, Some(1)),
    ("not requested", 3, None),
];

async fn readings<C>(executor: C) -> anyhow::Result<Vec<(&'static str, i64)>>
where
    C: ReadExecutor + Copy,
{
    let no_ids = Vec::<String>::new;
    let values: [i64; READINGS.len()] = [
        count::<Store>().execute(executor).await?.total,
        count::<Store>().where_(|s| s.city.null()).execute(executor).await?.total,
        count::<Product>().where_(|p| p.archived.eq(false)).where_(|p| p.price.gte(100)).execute(executor).await?.total,
        count::<Product>()
            .where_complex(|p, m| m.or(p.price.lt(100), p.archived.eq(true)))
            .execute(executor)
            .await?
            .total,
        count::<Product>()
            .where_(|p| p.price.gt(1000))
            .where_complex(|p, m| m.and([p.archived.eq(false)]))
            .execute(executor)
            .await?
            .total,
        count::<Store>().where_(|s| s.id.batch(no_ids())).execute(executor).await?.total,
        count::<Store>().where_(|s| s.products.where_(|p| p.archived.eq(true))).execute(executor).await?.total,
        count::<Store>().where_(|s| s.products.every(|p| p.archived.eq(false))).execute(executor).await?.total,
        count::<Store>()
            .where_(|s| s.products.none_complex(|p, m| m.and([p.archived.eq(false), p.price.lt(200)])))
            .execute(executor)
            .await?
            .total,
        count::<Product>().where_(|p| p.store.where_(|s| s.city.eq("Recife"))).execute(executor).await?.total,
        count::<Product>().where_(|p| p.store.not_exists()).execute(executor).await?.total,
        count::<Store>().where_(|s| s.profile.where_(|pf| pf.verified.eq(true))).execute(executor).await?.total,
        count::<Profile>().where_(|pf| pf.store.where_(|s| s.city.null())).execute(executor).await?.total,
        count::<Product>().where_(|p| p.labels.where_(|l| l.name.eq("sale"))).execute(executor).await?.total,
        count::<Label>().where_(|l| l.products.not_exists()).execute(executor).await?.total,
        count::<Category>().where_(|c| c.children.exists()).execute(executor).await?.total,
        count::<Category>()
            .where_(|c| c.parent.where_(|parent| parent.parent_id.null()))
            .execute(executor)
            .await?
            .total,
        count::<Label>()
            .where_(|l| l.products.where_(|p| p.store.where_(|s| s.profile.where_(|pf| pf.verified.eq(true)))))
            .execute(executor)
            .await?
            .total,
        i64::from(exists::<Store>().where_(|s| s.name.eq("Gamma")).execute(executor).await?),
        i64::from(exists::<Store>().where_(|s| s.name.eq("Nope")).execute(executor).await?),
        i64::from(
            exists::<Product>()
                .where_(|p| p.archived.eq(true))
                .where_(|p| p.labels.where_(|l| l.name.eq("sale")))
                .execute(executor)
                .await?,
        ),
        i64::from(
            exists::<Product>()
                .where_complex(|p, m| m.or(p.price.lt(60), p.store.where_(|s| s.city.eq("Natal"))))
                .execute(executor)
                .await?,
        ),
        i64::from(
            exists::<Product>()
                .where_(|p| p.price.gt(1000))
                .where_complex(|p, m| m.not(p.archived.eq(false)))
                .execute(executor)
                .await?,
        ),
        i64::from(exists::<Store>().where_(|s| s.id.batch(no_ids())).execute(executor).await?),
        i64::from(exists::<Product>().where_(|p| p.store_id.null()).execute(executor).await?),
        i64::from(exists::<Store>().read_in_primary().execute(executor).await?),
    ];

    Ok(READINGS.iter().zip(values).map(|((label, _), value)| (*label, value)).collect())
}

async fn relation_counts<C>(executor: C) -> anyhow::Result<Vec<(&'static str, i64, Option<i64>)>>
where
    C: ReadExecutor + Copy,
{
    let stores: StoreCount = count::<Store>().includes(|s| s.products()).execute(executor).await?;
    let recife = count::<Store>()
        .where_(|s| s.city.eq("Recife"))
        .includes(|s| s.products().where_(|p| p.archived.eq(false)))
        .execute(executor)
        .await?;
    let gamma = count::<Store>().where_(|s| s.name.eq("Gamma")).includes(|s| s.products()).execute(executor).await?;
    let nope = count::<Store>().where_(|s| s.name.eq("Nope")).includes(|s| s.products()).execute(executor).await?;
    let labelled = count::<Store>().includes(|s| s.products().where_(|p| p.labels.exists())).execute(executor).await?;
    let products: ProductCount = count::<Product>().includes(|p| p.labels()).execute(executor).await?;
    let sale: LabelCount = count::<Label>()
        .where_(|l| l.name.eq("sale"))
        .includes(|l| l.products().where_(|p| p.price.gt(200)))
        .execute(executor)
        .await?;
    let empty = count::<Label>().where_(|l| l.name.eq("empty")).includes(|l| l.products()).execute(executor).await?;
    let root: CategoryCount =
        count::<Category>().where_(|c| c.id.eq("root")).includes(|c| c.children()).execute(executor).await?;
    let categories = count::<Category>().includes(|c| c.children()).execute(executor).await?;
    let complex = count::<Category>()
        .where_complex(|c, m| m.or(c.id.eq("a"), c.id.eq("orphan")))
        .includes(|c| c.children())
        .execute(executor)
        .await?;
    let plain = count::<Store>().execute(executor).await?;

    let values: [(i64, Option<i64>); RELATION_COUNTS.len()] = [
        (stores.total, stores.products),
        (recife.total, recife.products),
        (gamma.total, gamma.products),
        (nope.total, nope.products),
        (labelled.total, labelled.products),
        (products.total, products.labels),
        (sale.total, sale.products),
        (empty.total, empty.products),
        (root.total, root.children),
        (categories.total, categories.children),
        (complex.total, complex.children),
        (plain.total, plain.products),
    ];

    Ok(RELATION_COUNTS.iter().zip(values).map(|((label, ..), (total, related))| (*label, total, related)).collect())
}

fn expected_readings() -> Vec<(&'static str, i64)> {
    READINGS.to_vec()
}

fn expected_relation_counts() -> Vec<(&'static str, i64, Option<i64>)> {
    RELATION_COUNTS.to_vec()
}

async fn reads_match_the_client(client: DinocoClient) -> anyhow::Result<()> {
    assert_eq!(readings(&client).await?, expected_readings());

    let in_transaction = transaction(&client, |tx| async move { readings(tx).await }).await?;
    assert_eq!(in_transaction, expected_readings());

    let in_custom_transaction =
        transaction_with_error::<String, _>(&client, |tx| async move { Ok(readings(tx).await?) }).await?;
    assert_eq!(in_custom_transaction, expected_readings());

    // `count`/`exists` take the same executors as `find_many`: a shared client
    // and a reference to it work too.
    let shared = Arc::new(client);
    assert_eq!(count::<Store>().execute(Arc::clone(&shared)).await?.total, 3);
    assert!(exists::<Store>().execute(&shared).await?);

    Ok(())
}

async fn relation_counts_match_the_client(client: DinocoClient) -> anyhow::Result<()> {
    assert_eq!(relation_counts(&client).await?, expected_relation_counts());

    let in_transaction = transaction(&client, |tx| async move { relation_counts(tx).await }).await?;
    assert_eq!(in_transaction, expected_relation_counts());

    Ok(())
}

async fn sees_uncommitted_writes(client: DinocoClient) -> anyhow::Result<()> {
    let client = &client;
    let delta = &Store::new("s4".to_string(), "Delta".to_string());
    let products = &[product("p5", 10, false, Some("s4")), product("p6", 20, true, Some("s4"))];

    let result: Result<(), _> = transaction(client, |tx| async move {
        insert_into::<Store>().value(delta).execute(tx).await?;
        insert_many::<Product>().values(products).execute(tx).await?;
        tx.insert(links(&[("empty", "p5"), ("sale", "p6")])).await?;

        let stores: StoreCount = count::<Store>().includes(|s| s.products()).execute(tx).await?;
        assert_eq!((stores.total, stores.products), (4, Some(5)));
        let delta_products = count::<Store>().where_(|s| s.id.eq("s4")).includes(|s| s.products()).execute(tx).await?;
        assert_eq!((delta_products.total, delta_products.products), (1, Some(2)));
        let empty = count::<Label>().where_(|l| l.name.eq("empty")).includes(|l| l.products()).execute(tx).await?;
        assert_eq!(empty.products, Some(1));
        assert!(exists::<Store>().where_(|s| s.name.eq("Delta")).execute(tx).await?);
        assert!(exists::<Label>().where_(|l| l.name.eq("empty")).where_(|l| l.products.exists()).execute(tx).await?);
        assert_eq!(
            count::<Store>().where_(|s| s.products.where_(|p| p.archived.eq(true))).execute(tx).await?.total,
            2,
            "s1 and the new s4"
        );

        // Another connection only sees what is committed.
        assert_eq!(count::<Store>().execute(client).await?.total, 3);
        assert!(!exists::<Store>().where_(|s| s.name.eq("Delta")).execute(client).await?);
        assert_eq!(relation_counts(client).await?, expected_relation_counts());

        anyhow::bail!("roll back after counting")
    })
    .await;
    assert!(matches!(result, Err(TransactionError::Operation(_))), "{result:?}");
    assert_eq!(readings(client).await?, expected_readings());
    assert_eq!(relation_counts(client).await?, expected_relation_counts());

    let committed = transaction(client, |tx| async move {
        insert_into::<Store>().value(delta).execute(tx).await?;
        insert_many::<Product>().values(products).execute(tx).await?;
        let stores = count::<Store>().includes(|s| s.products()).execute(tx).await?;
        Ok((stores.total, stores.products, exists::<Store>().where_(|s| s.id.eq("s4")).execute(tx).await?))
    })
    .await?;
    assert_eq!(committed, (4, Some(5), true));
    let stores = count::<Store>().includes(|s| s.products()).execute(client).await?;
    assert_eq!((stores.total, stores.products), (4, Some(5)));
    assert!(exists::<Store>().where_(|s| s.id.eq("s4")).execute(client).await?);

    Ok(())
}

async fn every_write_builder(client: DinocoClient) -> anyhow::Result<()> {
    let client = &client;
    let gamma = &product("p5", 500, false, Some("s3"));
    let loose = &[product("p6", 5, true, None), product("p7", 6, true, None)];
    let gamma_has_products = || exists::<Store>().where_(|s| s.name.eq("Gamma")).where_(|s| s.products.exists());

    let result: Result<(), _> = transaction(client, |tx| async move {
        insert_into::<Product>().value(gamma).execute(tx).await?;
        assert_eq!(count::<Product>().where_(|p| p.archived.eq(false)).execute(tx).await?.total, 4);
        assert!(gamma_has_products().execute(tx).await?);

        insert_many::<Product>().values(loose).execute(tx).await?;
        assert_eq!(count::<Product>().execute(tx).await?.total, 7);
        assert_eq!(count::<Product>().where_(|p| p.store.not_exists()).execute(tx).await?.total, 3);

        update::<Product>().where_(|p| p.id.eq("p2")).update(|p| p.archived.set(false)).execute(tx).await?;
        assert!(!exists::<Store>().where_(|s| s.products.where_(|p| p.archived.eq(true))).execute(tx).await?);

        update_many::<Product>().where_(|p| p.store_id.null()).update(|p| p.price.increment(1000)).execute(tx).await?;
        assert_eq!(count::<Product>().where_(|p| p.price.gte(1000)).execute(tx).await?.total, 3);

        find_and_update::<Store>()
            .where_(|s| s.id.eq("s2"))
            .update(|s| s.city.set(Some("Natal".to_string())))
            .execute(tx)
            .await?;
        assert!(!exists::<Store>().where_(|s| s.city.null()).execute(tx).await?);
        assert_eq!(count::<Product>().where_(|p| p.store.where_(|s| s.city.eq("Natal"))).execute(tx).await?.total, 2);

        delete::<Product>().where_(|p| p.id.eq("p5")).execute(tx).await?;
        assert!(!gamma_has_products().execute(tx).await?);

        delete_many::<Product>().where_(|p| p.store_id.null()).execute(tx).await?;
        let stores = count::<Store>().includes(|s| s.products()).execute(tx).await?;
        assert_eq!((stores.total, stores.products), (3, Some(3)));
        assert!(!exists::<Product>().where_(|p| p.store.not_exists()).execute(tx).await?);

        anyhow::bail!("roll back every write")
    })
    .await;
    assert!(matches!(result, Err(TransactionError::Operation(_))), "{result:?}");
    assert_eq!(readings(client).await?, expected_readings());

    Ok(())
}

/// Commands sent at the same time share the transaction's connection.
async fn concurrent_reads(client: DinocoClient) -> anyhow::Result<()> {
    let label = &Label::new("fresh".to_string(), "fresh".to_string());

    let (stores, on_sale, labels, products) = transaction(&client, |tx| async move {
        insert_into::<Label>().value(label).execute(tx).await?;

        tokio::try_join!(
            count::<Store>().includes(|s| s.products()).execute(tx),
            exists::<Product>().where_(|p| p.labels.where_(|l| l.name.eq("sale"))).execute(tx),
            count::<Label>().includes(|l| l.products()).execute(tx),
            find_many::<Product>().execute(tx),
        )
    })
    .await?;
    assert_eq!((stores.total, stores.products), (3, Some(3)));
    assert!(on_sale);
    assert_eq!((labels.total, labels.products), (4, Some(3)));
    assert_eq!(products.len(), 4);
    assert_eq!(count::<Label>().execute(&client).await?.total, 4);

    Ok(())
}

async fn errors_roll_back(client: DinocoClient) -> anyhow::Result<()> {
    let client = &client;
    let delta = &Store::new("s4".to_string(), "Delta".to_string());

    let result: Result<(), _> = transaction(client, |tx| async move {
        insert_into::<Store>().value(delta).execute(tx).await?;
        count::<Missing>().execute(tx).await?;
        Ok(())
    })
    .await;
    assert!(matches!(result, Err(TransactionError::Operation(_))), "{result:?}");

    let result: Result<(), _> = transaction(client, |tx| async move {
        insert_into::<Store>().value(delta).execute(tx).await?;
        exists::<Missing>().where_(|missing| missing.id.eq("x")).execute(tx).await?;
        Ok(())
    })
    .await;
    assert!(matches!(result, Err(TransactionError::Operation(_))), "{result:?}");

    // A relation count that fails after the parent count succeeded.
    let result: Result<(), _> = transaction(client, |tx| async move {
        insert_into::<Store>().value(delta).execute(tx).await?;
        count::<Store>()
            .includes(|_| RelationCount::<Store, Missing>::new("missing", "id", "store_id"))
            .execute(tx)
            .await?;
        Ok(())
    })
    .await;
    assert!(matches!(result, Err(TransactionError::Operation(_))), "{result:?}");
    assert!(!exists::<Store>().where_(|s| s.id.eq("s4")).execute(client).await?);

    // A domain rule decided by a count rolls back like any other error.
    let result = transaction_with_error::<&'static str, _>(client, |tx| async move {
        insert_into::<Store>().value(delta).execute(tx).await?;
        let stores = count::<Store>().execute(tx).await?;
        if stores.total > 3 && exists::<Store>().where_(|s| s.id.eq("s4")).execute(tx).await? {
            return Err(TransactionError::Custom("too many stores"));
        }
        Ok(stores.total)
    })
    .await;
    assert!(matches!(result, Err(TransactionError::Custom("too many stores"))), "{result:?}");
    assert_eq!(readings(client).await?, expected_readings());

    let leaked = transaction(client, |tx| async move { Ok(tx) }).await?;
    let error = count::<Store>().execute(leaked).await.expect_err("count outside its closure");
    assert!(error.to_string().contains("outside its transaction closure"), "{error}");
    let error = exists::<Store>().execute(leaked).await.expect_err("exists outside its closure");
    assert!(error.to_string().contains("outside its transaction closure"), "{error}");
    let error = count::<Store>().includes(|s| s.products()).execute(leaked).await.expect_err("relation count");
    assert!(error.to_string().contains("outside its transaction closure"), "{error}");

    Ok(())
}

/// `count` and `exists` report to no hook, on the client or in a transaction.
async fn reports_no_hooks(client: DinocoClient) -> anyhow::Result<()> {
    let finds = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&finds);
    setup_test_methods::<Store>(&client).on_find(move |_, _, _| {
        counter.fetch_add(1, Ordering::SeqCst);
    });

    transaction(&client, |tx| async move {
        count::<Store>().where_(|s| s.city.null()).execute(tx).await?;
        count::<Store>().includes(|s| s.products()).execute(tx).await?;
        exists::<Store>().execute(tx).await?;
        assert_eq!(find_many::<Store>().execute(tx).await?.len(), 3);
        Ok(())
    })
    .await?;
    count::<Store>().execute(&client).await?;
    exists::<Store>().execute(&client).await?;
    assert_eq!(finds.load(Ordering::SeqCst), 1, "only find_many reports to on_find");

    remove_test_methods::<Store>(&client);
    Ok(())
}

#[allow(dead_code)]
fn futures_are_send(client: &DinocoClient) {
    fn assert_send<T: Send>(_: T) {}

    assert_send(count::<Store>().execute(client));
    assert_send(count::<Store>().execute(TransactionContext));
    assert_send(count::<Store>().includes(|s| s.products()).execute(TransactionContext));
    assert_send(exists::<Store>().execute(client));
    assert_send(exists::<Store>().where_(|s| s.products.exists()).execute(TransactionContext));
    assert_send(transaction(client, |tx| async move {
        Ok((count::<Store>().execute(tx).await?, exists::<Store>().execute(tx).await?))
    }));
}

fn product(id: &str, price: i64, archived: bool, store_id: Option<&str>) -> Product {
    let mut product = Product::new(id.to_string(), price, archived);
    product.store_id = store_id.map(str::to_string);
    product
}

fn links(pairs: &[(&str, &str)]) -> InsertQuery {
    InsertQuery {
        table: "_ce_label_to_product",
        fields: vec!["label_id", "product_id"],
        rows: pairs
            .iter()
            .map(|(label, product)| {
                vec![DinocoValue::String(label.to_string()), DinocoValue::String(product.to_string())]
            })
            .collect(),
        returning: None,
        on_conflict_do_nothing: None,
    }
}

async fn run_on_sqlite<F, Fut>(name: &str, scenario: F) -> anyhow::Result<()>
where
    F: FnOnce(DinocoClient) -> Fut,
    Fut: Future<Output = anyhow::Result<()>>,
{
    let path = format!("/private/tmp/dinoco-ce-{name}-{}-{}.sqlite", std::process::id(), monotonic());
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

/// Drops every table, `ce_missing` included, and creates all but that one.
async fn reset_schema<A>(adapter: &A) -> anyhow::Result<()>
where
    A: DinocoAdapter + DinocoSqlCompiler,
{
    for table in TABLES.iter().rev() {
        drop_table(adapter, table).await?;
    }

    let string = |name: &str| column(name, MigrationColumnType::String);
    create_table(adapter, "ce_store", vec![primary(string("id")), string("name"), nullable(string("city"))]).await?;
    create_table(
        adapter,
        "ce_profile",
        vec![primary(string("id")), string("store_id"), column("verified", MigrationColumnType::Boolean)],
    )
    .await?;
    create_table(
        adapter,
        "ce_product",
        vec![
            primary(string("id")),
            column("price", MigrationColumnType::Integer),
            column("archived", MigrationColumnType::Boolean),
            nullable(string("store_id")),
        ],
    )
    .await?;
    create_table(adapter, "ce_label", vec![primary(string("id")), string("name")]).await?;
    create_table(adapter, "_ce_label_to_product", vec![string("label_id"), string("product_id")]).await?;
    create_table(adapter, "ce_category", vec![primary(string("id")), nullable(string("parent_id"))]).await
}

/// Seeds:
///
/// | store     | city   | profile (verified) | products (price, archived) |
/// | --------- | ------ | ------------------ | -------------------------- |
/// | s1 Alpha  | Recife | pf1 (yes)          | p1 (100, no), p2 (250, yes) |
/// | s2 Beta   | NULL   | pf2 (no)           | p3 (400, no)               |
/// | s3 Gamma  | Natal  |                    |                            |
///
/// p4 (50, no) has no store. Labels: sale (p1, p3), new (p1), empty (none).
/// Categories: root -> {a -> a1, b}; orphan has no parent and no children.
async fn seed(client: &DinocoClient) -> anyhow::Result<()> {
    let store = |id: &str, name: &str, city: Option<&str>| {
        let mut store = Store::new(id.to_string(), name.to_string());
        store.city = city.map(str::to_string);
        store
    };
    insert_many::<Store>()
        .values(vec![
            store("s1", "Alpha", Some("Recife")),
            store("s2", "Beta", None),
            store("s3", "Gamma", Some("Natal")),
        ])
        .execute(client)
        .await?;
    insert_many::<Profile>()
        .values(vec![
            Profile::new("pf1".to_string(), "s1".to_string(), true),
            Profile::new("pf2".to_string(), "s2".to_string(), false),
        ])
        .execute(client)
        .await?;
    insert_many::<Product>()
        .values(vec![
            product("p1", 100, false, Some("s1")),
            product("p2", 250, true, Some("s1")),
            product("p3", 400, false, Some("s2")),
            product("p4", 50, false, None),
        ])
        .execute(client)
        .await?;
    insert_many::<Label>()
        .values(["sale", "new", "empty"].map(|name| Label::new(name.to_string(), name.to_string())))
        .execute(client)
        .await?;
    client.backend.insert(links(&[("sale", "p1"), ("new", "p1"), ("sale", "p3")])).await?;

    let category = |id: &str, parent_id: Option<&str>| {
        let mut category = Category::new(id.to_string());
        category.parent_id = parent_id.map(str::to_string);
        category
    };
    insert_many::<Category>()
        .values(vec![
            category("root", None),
            category("a", Some("root")),
            category("b", Some("root")),
            category("a1", Some("a")),
            category("orphan", None),
        ])
        .execute(client)
        .await?;

    Ok(())
}

fn monotonic() -> u128 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
}
