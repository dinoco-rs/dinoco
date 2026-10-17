//! Includes and relation counts keyed by every column type a relation key can
//! have. PostgreSQL types an unannotated parameter inside `VALUES` as `text`,
//! so the parent keys of a relation query have to be cast to the type of the
//! value they hold, or `fee.group_id = __dinoco_relation_key` fails with
//! `operator does not exist: bigint = text`. SQLite, which has dynamic types,
//! never showed it.
//!
//! Every scenario runs on SQLite, PostgreSQL and MySQL.

use std::future::Future;
use std::sync::{Arc, Mutex};

use dinoco::{Entity, count, find_first, find_many, remove_test_methods, setup_test_methods, transaction};
use dinoco_engine::{
    Backend, DinocoAdapter, DinocoClient, DinocoSqlCompiler, DinocoValue, InsertQuery, MigrationColumnType,
    MySqlAdapter, PostgresAdapter, SqliteAdapter,
};
use dinoco_tests::{column, create_table, drop_table, nullable, primary};

const POSTGRES_URL: &str = "postgres://postgres:postgres@localhost:5432/postgres";
const MYSQL_URL: &str = "mysql://root:root@localhost:3306/mysql";
static POSTGRES_TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
static MYSQL_TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

const TABLES: [&str; 6] = ["rk_group", "rk_fee", "rk_part", "rk_tag", "_rk_fee_to_rk_tag", "rk_note"];

/// Snowflake-sized ids: they only fit a `BIGINT`.
const G1: i64 = 7_300_000_000_000_000_001;
const G2: i64 = 7_300_000_000_000_000_002;
const G3: i64 = 7_300_000_000_000_000_003;
const G4: i64 = 7_300_000_000_000_000_004;

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
    nested_has_many,
    one_parent_key,
    belongs_to,
    many_to_many,
    take_and_skip,
    text_keys,
    relation_counts,
    inside_a_transaction,
    hooks_see_the_sql_that_ran,
);

#[derive(Debug, Entity)]
#[dinoco(table_name = "rk_group")]
pub struct FeeGroup {
    #[dinoco(primary_key)]
    id: i64,
    name: String,
    business_id: Option<i64>,

    #[dinoco(one_to_many, foreign_key = "group_id", references = "id")]
    fees: Vec<Fee>,
}

#[derive(Debug, Entity)]
#[dinoco(table_name = "rk_fee")]
pub struct Fee {
    #[dinoco(primary_key)]
    id: i64,
    group_id: Option<i64>,
    amount: i64,

    #[dinoco(many_to_one, foreign_key = "group_id", references = "id")]
    group: Option<FeeGroup>,

    #[dinoco(one_to_many, foreign_key = "fee_id", references = "id")]
    parts: Vec<Part>,

    #[dinoco(
        many_to_many,
        foreign_key = "id",
        references = "id",
        join_table = "_rk_fee_to_rk_tag",
        parent_field = "id",
        join_parent_field = "fee_id",
        join_child_field = "tag_id"
    )]
    tags: Vec<Tag>,
}

#[derive(Debug, Entity)]
#[dinoco(table_name = "rk_part")]
pub struct Part {
    #[dinoco(primary_key)]
    id: i64,
    fee_id: i64,
    label: String,

    #[dinoco(many_to_one, foreign_key = "fee_id", references = "id")]
    fee: Option<Fee>,
}

#[derive(Debug, Entity)]
#[dinoco(table_name = "rk_tag")]
pub struct Tag {
    #[dinoco(primary_key)]
    id: i64,
    name: String,

    #[dinoco(
        many_to_many,
        foreign_key = "id",
        references = "id",
        join_table = "_rk_fee_to_rk_tag",
        parent_field = "id",
        join_parent_field = "tag_id",
        join_child_field = "fee_id"
    )]
    fees: Vec<Fee>,
}

/// Keyed by a text column, the group's name.
#[derive(Debug, Entity)]
#[dinoco(table_name = "rk_note")]
pub struct Note {
    #[dinoco(primary_key)]
    id: String,
    group_name: String,

    #[dinoco(many_to_one, foreign_key = "group_name", references = "name")]
    group: Option<NamedGroup>,
}

#[derive(Debug, Entity)]
#[dinoco(table_name = "rk_group")]
pub struct NamedGroup {
    #[dinoco(primary_key)]
    id: i64,
    name: String,

    #[dinoco(one_to_many, foreign_key = "group_name", references = "name")]
    notes: Vec<Note>,
}

/// The query from the report: groups without a business, their fees and the
/// fees' parts, grouped per parent.
async fn nested_has_many(client: DinocoClient) -> anyhow::Result<()> {
    let groups = find_many::<FeeGroup>()
        .where_(|x| x.business_id.null())
        .includes(|x| x.fees().includes(|x| x.parts().order_by(|part| part.id.asc())).order_by(|fee| fee.id.asc()))
        .order_by(|group| group.id.asc())
        .execute(&client)
        .await?;

    assert_eq!(tree(&groups), expected_tree());
    Ok(())
}

async fn one_parent_key(client: DinocoClient) -> anyhow::Result<()> {
    let group = find_first::<FeeGroup>()
        .where_(|x| x.id.eq(G1))
        .includes(|x| x.fees().includes(|x| x.parts().order_by(|part| part.id.asc())).order_by(|fee| fee.id.asc()))
        .execute(&client)
        .await?
        .expect("g1");
    assert_eq!(tree(std::slice::from_ref(&group)), expected_tree()[..1]);

    let empty = find_first::<FeeGroup>().where_(|x| x.id.eq(G3)).includes(|x| x.fees()).execute(&client).await?;
    assert!(empty.expect("g3").fees.is_empty(), "a group without fees loads none");
    Ok(())
}

async fn belongs_to(client: DinocoClient) -> anyhow::Result<()> {
    let parts = find_many::<Part>()
        .includes(|x| x.fee().includes(|x| x.group()))
        .order_by(|part| part.id.asc())
        .execute(&client)
        .await?;
    let loaded = parts
        .iter()
        .map(|part| {
            let fee = part.fee.as_ref().expect("every part has a fee");
            (part.id, fee.id, fee.group.as_ref().map(|group| group.id))
        })
        .collect::<Vec<_>>();
    assert_eq!(loaded, [(1, 11, Some(G1)), (2, 11, Some(G1)), (3, 12, Some(G1)), (4, 21, Some(G2)), (5, 31, Some(G4))]);
    Ok(())
}

async fn many_to_many(client: DinocoClient) -> anyhow::Result<()> {
    let fees = find_many::<Fee>()
        .includes(|x| x.tags().order_by(|tag| tag.id.asc()))
        .order_by(|fee| fee.id.asc())
        .execute(&client)
        .await?;
    let loaded = fees.iter().map(|fee| (fee.id, fee.tags.iter().map(|tag| tag.id).collect())).collect::<Vec<_>>();
    assert_eq!(loaded, [(11, vec![G1, G2]), (12, vec![G2]), (21, vec![]), (31, vec![G1])]);

    let tags = find_many::<Tag>()
        .includes(|x| x.fees().includes(|x| x.parts().order_by(|part| part.id.asc())).order_by(|fee| fee.id.asc()))
        .order_by(|tag| tag.id.asc())
        .execute(&client)
        .await?;
    let loaded = tags
        .iter()
        .map(|tag| (tag.id, tag.fees.iter().map(|fee| (fee.id, fee.parts.len())).collect()))
        .collect::<Vec<(i64, Vec<(i64, usize)>)>>();
    assert_eq!(loaded, [(G1, vec![(11, 2), (31, 1)]), (G2, vec![(11, 2), (12, 1)])]);
    Ok(())
}

/// `take`/`skip` switch includes to their partitioned (`ROW_NUMBER`) queries.
async fn take_and_skip(client: DinocoClient) -> anyhow::Result<()> {
    let groups = find_many::<FeeGroup>()
        .includes(|x| x.fees().order_by(|fee| fee.amount.desc()).take(1))
        .order_by(|group| group.id.asc())
        .execute(&client)
        .await?;
    let loaded = groups.iter().map(|group| fee_ids(&group.fees)).collect::<Vec<_>>();
    assert_eq!(loaded, [vec![12], vec![21], vec![], vec![31]]);

    let groups = find_many::<FeeGroup>()
        .where_(|x| x.id.eq(G1))
        .includes(|x| x.fees().order_by(|fee| fee.id.asc()).skip(1))
        .execute(&client)
        .await?;
    assert_eq!(fee_ids(&groups[0].fees), [12]);

    let fees = find_many::<Fee>()
        .includes(|x| x.tags().order_by(|tag| tag.id.desc()).take(1))
        .includes(|x| x.parts().order_by(|part| part.id.desc()).skip(1).take(1))
        .order_by(|fee| fee.id.asc())
        .execute(&client)
        .await?;
    let loaded = fees
        .iter()
        .map(|fee| {
            (fee.id, fee.tags.iter().map(|tag| tag.id).collect(), fee.parts.iter().map(|part| part.id).collect())
        })
        .collect::<Vec<(i64, Vec<i64>, Vec<i64>)>>();
    assert_eq!(loaded, [(11, vec![G2], vec![1]), (12, vec![G2], vec![]), (21, vec![], vec![]), (31, vec![G1], vec![])]);
    Ok(())
}

/// The cast follows the key's value, so a text key is not cast to `bigint`.
async fn text_keys(client: DinocoClient) -> anyhow::Result<()> {
    let groups = find_many::<NamedGroup>()
        .includes(|x| x.notes().order_by(|note| note.id.asc()))
        .order_by(|group| group.id.asc())
        .execute(&client)
        .await?;
    let loaded = groups
        .iter()
        .map(|group| (group.name.as_str(), group.notes.iter().map(|note| note.id.as_str()).collect()))
        .collect::<Vec<(&str, Vec<&str>)>>();
    assert_eq!(loaded, [("alpha", vec!["n1", "n2"]), ("beta", vec![]), ("gamma", vec!["n3"]), ("delta", vec![])]);

    let notes = find_many::<Note>().includes(|x| x.group()).order_by(|note| note.id.asc()).execute(&client).await?;
    let loaded = notes.iter().map(|note| note.group.as_ref().map(|group| group.id)).collect::<Vec<_>>();
    assert_eq!(loaded, [Some(G1), Some(G1), Some(G3)]);
    Ok(())
}

async fn relation_counts(client: DinocoClient) -> anyhow::Result<()> {
    let counted = count::<FeeGroup>().includes(|x| x.fees()).execute(&client).await?;
    assert_eq!((counted.total, counted.fees), (4, Some(4)));

    let counted = count::<Fee>()
        .where_(|x| x.group_id.eq(G1))
        .includes(|x| x.parts())
        .includes(|x| x.tags())
        .execute(&client)
        .await?;
    assert_eq!((counted.total, counted.parts, counted.tags), (2, Some(3), Some(3)));
    Ok(())
}

async fn inside_a_transaction(client: DinocoClient) -> anyhow::Result<()> {
    let (groups, first_fees) = transaction(&client, |tx| async move {
        let groups = find_many::<FeeGroup>()
            .where_(|x| x.business_id.null())
            .includes(|x| x.fees().includes(|x| x.parts().order_by(|part| part.id.asc())).order_by(|fee| fee.id.asc()))
            .order_by(|group| group.id.asc())
            .execute(tx)
            .await?;
        let first_fees = find_many::<FeeGroup>()
            .includes(|x| x.fees().order_by(|fee| fee.id.asc()).take(1))
            .order_by(|group| group.id.asc())
            .execute(tx)
            .await?;
        Ok((groups, first_fees))
    })
    .await?;
    assert_eq!(tree(&groups), expected_tree());
    let loaded = first_fees.iter().map(|group| fee_ids(&group.fees)).collect::<Vec<_>>();
    assert_eq!(loaded, [vec![11], vec![21], vec![], vec![31]]);
    Ok(())
}

/// The SQL a hook receives for an include is the SQL that ran, casts included.
async fn hooks_see_the_sql_that_ran(client: DinocoClient) -> anyhow::Result<()> {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let recorder = Arc::clone(&seen);
    setup_test_methods::<Fee>(&client).on_find(move |rows, executed, error| {
        assert!(error.is_none(), "{error:?}");
        recorder.lock().expect("hook log").push((executed.sql.clone(), executed.params.clone(), rows));
    });

    let groups = find_many::<FeeGroup>()
        .where_(|x| x.business_id.null())
        .includes(|x| x.fees().order_by(|fee| fee.id.asc()))
        .order_by(|group| group.id.asc())
        .execute(&client)
        .await;
    remove_test_methods::<Fee>(&client);
    let groups = groups?;
    assert_eq!(groups.iter().map(|group| fee_ids(&group.fees)).collect::<Vec<_>>(), [vec![11, 12], vec![], vec![31]]);

    let seen = seen.lock().expect("hook log");
    let [(sql, params, rows)] = seen.as_slice() else { panic!("one include query: {seen:?}") };
    assert!(sql.contains("__dinoco_parent_keys"), "{sql}");
    if sql.contains("$1") {
        assert!(sql.contains("(0, $1::bigint), (1, $2::bigint), (2, $3::bigint)"), "{sql}");
    }
    assert_eq!(params[..3], [DinocoValue::Integer(G1), DinocoValue::Integer(G3), DinocoValue::Integer(G4)]);
    assert_eq!(*rows, Some(3));
    Ok(())
}

type Tree = Vec<(i64, Vec<(i64, Vec<i64>)>)>;

fn tree(groups: &[FeeGroup]) -> Tree {
    groups
        .iter()
        .map(|group| {
            (group.id, group.fees.iter().map(|fee| (fee.id, fee.parts.iter().map(|part| part.id).collect())).collect())
        })
        .collect()
}

/// Groups without a business: g1, g3 and g4 (g2 belongs to one).
fn expected_tree() -> Tree {
    vec![(G1, vec![(11, vec![1, 2]), (12, vec![3])]), (G3, vec![]), (G4, vec![(31, vec![5])])]
}

fn fee_ids(fees: &[Fee]) -> Vec<i64> {
    fees.iter().map(|fee| fee.id).collect()
}

async fn run_on_sqlite<F, Fut>(name: &str, scenario: F) -> anyhow::Result<()>
where
    F: FnOnce(DinocoClient) -> Fut,
    Fut: Future<Output = anyhow::Result<()>>,
{
    let path =
        std::env::temp_dir().join(format!("dinoco-key-types-{name}-{}-{}.sqlite", std::process::id(), monotonic()));
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

    let integer = |name: &str| column(name, MigrationColumnType::Integer);
    let string = |name: &str| column(name, MigrationColumnType::String);
    create_table(adapter, "rk_group", vec![primary(integer("id")), string("name"), nullable(integer("business_id"))])
        .await?;
    create_table(adapter, "rk_fee", vec![primary(integer("id")), nullable(integer("group_id")), integer("amount")])
        .await?;
    create_table(adapter, "rk_part", vec![primary(integer("id")), integer("fee_id"), string("label")]).await?;
    create_table(adapter, "rk_tag", vec![primary(integer("id")), string("name")]).await?;
    create_table(adapter, "_rk_fee_to_rk_tag", vec![integer("fee_id"), integer("tag_id")]).await?;
    create_table(adapter, "rk_note", vec![primary(string("id")), string("group_name")]).await
}

/// Seeds:
///
/// | group | name  | business | fees (amount): parts          | tags of the fee |
/// | ----- | ----- | -------- | ----------------------------- | --------------- |
/// | G1    | alpha | -        | 11 (10): 1, 2; 12 (20): 3     | 11: G1, G2; 12: G2 |
/// | G2    | beta  | 900      | 21 (5): 4                     |                 |
/// | G3    | gamma | -        |                               |                 |
/// | G4    | delta | -        | 31 (7): 5                     | 31: G1          |
///
/// Tags reuse the group ids. Notes n1, n2 belong to `alpha`, n3 to `gamma`.
async fn seed(client: &DinocoClient) -> anyhow::Result<()> {
    let insert = |table: &'static str, fields: Vec<&'static str>, rows: Vec<Vec<DinocoValue>>| {
        client.backend.insert(InsertQuery { table, fields, rows, returning: None })
    };
    let int = DinocoValue::Integer;
    let text = |value: &str| DinocoValue::String(value.to_string());

    insert(
        "rk_group",
        vec!["id", "name", "business_id"],
        vec![
            vec![int(G1), text("alpha"), DinocoValue::Null],
            vec![int(G2), text("beta"), int(900)],
            vec![int(G3), text("gamma"), DinocoValue::Null],
            vec![int(G4), text("delta"), DinocoValue::Null],
        ],
    )
    .await?;
    insert(
        "rk_fee",
        vec!["id", "group_id", "amount"],
        vec![
            vec![int(11), int(G1), int(10)],
            vec![int(12), int(G1), int(20)],
            vec![int(21), int(G2), int(5)],
            vec![int(31), int(G4), int(7)],
        ],
    )
    .await?;
    insert(
        "rk_part",
        vec!["id", "fee_id", "label"],
        (1..=5).zip([11, 11, 12, 21, 31]).map(|(id, fee)| vec![int(id), int(fee), text("part")]).collect(),
    )
    .await?;
    insert("rk_tag", vec!["id", "name"], vec![vec![int(G1), text("vip")], vec![int(G2), text("promo")]]).await?;
    insert(
        "_rk_fee_to_rk_tag",
        vec!["fee_id", "tag_id"],
        vec![vec![int(11), int(G1)], vec![int(11), int(G2)], vec![int(12), int(G2)], vec![int(31), int(G1)]],
    )
    .await?;
    insert(
        "rk_note",
        vec!["id", "group_name"],
        vec![vec![text("n1"), text("alpha")], vec![text("n2"), text("alpha")], vec![text("n3"), text("gamma")]],
    )
    .await?;
    Ok(())
}

fn monotonic() -> u128 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
}
