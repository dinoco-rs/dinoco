//! Memory report: type sizes, future sizes, heap allocations and bytes per
//! operation, and peak live heap of large operations.
//!
//! Allocations are counted by a global allocator across every thread, so the
//! numbers include driver work; the `raw_driver` rows give the driver's own
//! share for the same read.

use std::future::Future;
use std::mem::{align_of, size_of};

use dinoco::{TransactionContext, count, exists, find_first, find_many, insert_into, insert_many, transaction, update};
use dinoco_benchmarks::*;
use dinoco_engine::*;

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

const ITERATIONS: u64 = 40;

macro_rules! layout {
    ($($ty:ty),* $(,)?) => {
        $(println!("| `{}` | {} | {} |", stringify!($ty), size_of::<$ty>(), align_of::<$ty>());)*
    };
}

fn future_size<F: Future>(name: &str, future: F) {
    println!("| {name} | {} |", std::mem::size_of_val(&future));
}

async fn measure<F, Fut, T>(name: &str, mut operation: F)
where
    F: FnMut() -> Fut,
    Fut: Future<Output = T>,
{
    for _ in 0..3 {
        drop(operation().await);
    }

    let before = snapshot();
    for _ in 0..ITERATIONS {
        drop(operation().await);
    }
    let after = snapshot();

    let base = reset_peak();
    let value = operation().await;
    let peak = peak().saturating_sub(base);
    drop(value);

    println!(
        "| {name} | {:.1} | {:.0} | {} |",
        (after.allocations - before.allocations) as f64 / ITERATIONS as f64,
        (after.bytes - before.bytes) as f64 / ITERATIONS as f64,
        peak,
    );
}

fn print_layouts() {
    println!("## Type layouts (bytes)\n");
    println!("| type | size | align |\n|---|---|---|");
    layout!(
        DinocoValue,
        FindWhere,
        FindQuery,
        FindOrderBy,
        InsertQuery,
        UpdateSet,
        UpdateOperation,
        UpdateQuery,
        DeleteQuery,
        CountQuery,
        RelationMatch,
        ManyToManyMatch,
        RelationOccurrenceQuery,
        ManyToManyRelationQuery,
        Backend,
        DinocoClient,
        SqliteAdapter,
        PostgresAdapter,
        MySqlAdapter,
        TransactionExecutor,
        TransactionCommand,
        DatabaseError,
        ExecutedQuery,
        dinoco::Field<String>,
        dinoco::UpdateField<String>,
        dinoco::RelationField<BenchPost>,
        BenchUserWhere,
        BenchUserUpdate,
        BenchUser,
        BenchPost,
        dinoco::FindMany<BenchUser>,
        dinoco::FindFirst<BenchUser>,
        dinoco::HasMany<BenchUser, BenchPost>,
        dinoco::BelongsTo<BenchPost, BenchUser>,
        dinoco::Update<BenchUser>,
        dinoco::InsertMany<BenchEvent>,
        dinoco::Insert<BenchEvent>,
        AggregateQuery,
        LookupQuery,
        dinoco::PluckAggregate<BenchUser, i64>,
        dinoco::PluckGroup<BenchUser, chrono::NaiveDate, i64>,
        dinoco::CreateError,
        dinoco::UpdateError,
        dinoco::TransactionError,
        anyhow::Error,
    );
}

fn print_future_sizes(client: &DinocoClient) {
    println!("\n## Future sizes (bytes, not awaited)\n");
    println!("| future | size |\n|---|---|");
    let item = event(0);
    future_size("find_many::<User>().execute", find_many::<BenchUser>().execute(client));
    future_size(
        "find_many::<User>().includes(posts).execute",
        find_many::<BenchUser>().includes(|user| user.posts()).execute(client),
    );
    future_size("find_first::<User>().execute", find_first::<BenchUser>().execute(client));
    future_size(
        "insert_into::<Event>().values(&item).execute",
        insert_into::<BenchEvent>().values(&item).execute(client),
    );
    future_size("insert_many::<Event>().execute", insert_many::<BenchEvent>().values(vec![event(1)]).execute(client));
    future_size(
        "update::<User>().execute",
        update::<BenchUser>().where_(|user| user.id.eq("x")).update(|user| user.age.increment(1)).execute(client),
    );
    future_size("count::<User>().execute", count::<BenchUser>().execute(client));
    future_size(
        "count::<User>().includes(posts).execute",
        count::<BenchUser>().includes(|user| user.posts()).execute(client),
    );
    future_size("exists::<User>().execute", exists::<BenchUser>().execute(client));
    future_size("count::<User>().execute(tx)", count::<BenchUser>().execute(TransactionContext));
    future_size("exists::<User>().execute(tx)", exists::<BenchUser>().execute(TransactionContext));
    future_size(
        "transaction(client, find_first)",
        transaction(client, |tx| async move { find_first::<BenchUser>().execute(tx).await }),
    );
    future_size(
        "find_many::<User>().pluck(age.sum()).execute",
        find_many::<BenchUser>().pluck(|user| user.age.sum()).execute(client),
    );
    future_size(
        "find_many::<User>().pluck(age.sum()).group_by(day).execute",
        find_many::<BenchUser>().pluck(|user| user.age.sum()).group_by(|user| user.created_at.day()).execute(client),
    );
    future_size(
        "insert_into::<Event>().on_conflict_do_nothing.execute",
        insert_into::<BenchEvent>().values(&item).on_conflict_do_nothing(|event| event.id).execute(client),
    );
    future_size(
        "insert_into::<Event>().on_conflict_do_nothing.returning.execute",
        insert_into::<BenchEvent>()
            .values(&item)
            .on_conflict_do_nothing(|event| event.id)
            .returning::<BenchEvent>()
            .execute(client),
    );
    future_size(
        "insert_into::<Event>().returning.execute",
        insert_into::<BenchEvent>().values(&item).returning::<BenchEvent>().execute(client),
    );
    future_size(
        "insert_many::<Event>().returning.execute",
        insert_many::<BenchEvent>().values(vec![event(1)]).returning::<BenchEvent>().execute(client),
    );
    future_size(
        "find_many::<User>().pluck(id).execute",
        find_many::<BenchUser>().pluck(|user| user.id).execute(client),
    );
    future_size(
        "Backend::query::<User>",
        client.backend.query::<BenchUser>(FindQuery::new(<BenchUser as DinocoEntity>::FIELDS, "bench_user", 1, -1)),
    );
}

async fn print_operations(fixture: &Fixture) {
    let client = &fixture.client;
    println!("\n## {} operations ({ITERATIONS} iterations)\n", fixture.dialect.name());
    println!("| operation | allocations/op | bytes/op | peak live bytes |\n|---|---|---|---|");

    for limit in [1usize, 100, 1000] {
        measure(&format!("find_many take({limit})"), || async move {
            find_many::<BenchUser>().order_by(|user| user.id.asc()).take(limit as i32).execute(client).await.unwrap()
        })
        .await;
        measure(&format!("raw_driver take({limit})"), || async move { raw_users(&fixture.raw, limit).await.unwrap() })
            .await;
    }
    measure("find_first by id", || async move {
        find_first::<BenchUser>().where_(|user| user.id.eq("u-000042")).execute(client).await.unwrap()
    })
    .await;
    measure("find_many complex where take(50)", || async move {
        find_many::<BenchUser>()
            .where_(|user| user.age.gte(21))
            .where_(|user| user.active.eq(true))
            .where_(|user| user.posts.where_(|post| post.published.eq(true)))
            .order_by(|user| user.created_at.desc())
            .take(50)
            .execute(client)
            .await
            .unwrap()
    })
    .await;
    measure("includes has_many (100 users)", || async move {
        find_many::<BenchUser>()
            .order_by(|user| user.id.asc())
            .take(100)
            .includes(|user| user.posts())
            .execute(client)
            .await
            .unwrap()
    })
    .await;
    measure("includes belongs_to (500 posts)", || async move {
        find_many::<BenchPost>()
            .order_by(|post| post.id.asc())
            .take(500)
            .includes(|post| post.user())
            .execute(client)
            .await
            .unwrap()
    })
    .await;
    measure("pluck ids take(1000)", || async move {
        find_many::<BenchUser>().take(1000).pluck(|user| user.id).execute(client).await.unwrap()
    })
    .await;
    measure("pluck age.sum() where", || async move {
        find_many::<BenchUser>()
            .where_(|user| user.active.eq(true))
            .pluck(|user| user.age.sum())
            .execute(client)
            .await
            .unwrap()
    })
    .await;
    measure("pluck age.sum() group_by(created_at.day())", || async move {
        find_many::<BenchUser>()
            .pluck(|user| user.age.sum())
            .group_by(|user| user.created_at.day())
            .execute(client)
            .await
            .unwrap()
    })
    .await;
    measure("count where", || async move {
        count::<BenchUser>().where_(|user| user.active.eq(true)).execute(client).await.unwrap()
    })
    .await;
    measure("count where includes has_many", || async move {
        count::<BenchUser>()
            .where_(|user| user.active.eq(true))
            .includes(|user| user.posts().where_(|post| post.published.eq(true)))
            .execute(client)
            .await
            .unwrap()
    })
    .await;
    measure("exists by id", || async move {
        exists::<BenchUser>().where_(|user| user.id.eq("u-000042")).execute(client).await.unwrap()
    })
    .await;
    measure("update by id", || async move {
        update::<BenchUser>()
            .where_(|user| user.id.eq("u-000007"))
            .update(|user| user.score.increment(1.0))
            .execute(client)
            .await
            .unwrap()
    })
    .await;
    measure("transaction (find_first, update, find_many)", || async move {
        transaction(client, |tx| async move {
            let user = find_first::<BenchUser>().where_(|user| user.id.eq("u-000011")).execute(tx).await?;
            update::<BenchUser>()
                .where_(|user| user.id.eq("u-000011"))
                .update(|user| user.age.increment(1))
                .execute(tx)
                .await?;
            let posts = find_many::<BenchPost>().where_(|post| post.user_id.eq("u-000011")).execute(tx).await?;
            Ok((user, posts))
        })
        .await
        .unwrap()
    })
    .await;

    measure("transaction (count includes, exists)", || async move {
        transaction(client, |tx| async move {
            let users = count::<BenchUser>()
                .where_(|user| user.active.eq(true))
                .includes(|user| user.posts())
                .execute(tx)
                .await?;
            let found = exists::<BenchUser>().where_(|user| user.id.eq("u-000042")).execute(tx).await?;
            Ok((users, found))
        })
        .await
        .unwrap()
    })
    .await;

    let mut next = 0usize;
    let mut take = |rows: usize| {
        let range = next..next + rows;
        next += rows;
        range
    };
    let ranges = (0..ITERATIONS + 4).map(|_| take(1)).collect::<Vec<_>>();
    let mut ranges = ranges.into_iter();
    measure("insert_into one (by reference)", || {
        let item = event(ranges.next().unwrap().start);
        async move { insert_into::<BenchEvent>().values(&item).execute(client).await.unwrap() }
    })
    .await;
    let ranges = (0..ITERATIONS + 4).map(|_| take(1)).collect::<Vec<_>>();
    let mut ranges = ranges.into_iter();
    measure("insert_into one (owned)", || {
        let item = event(ranges.next().unwrap().start);
        async move { insert_into::<BenchEvent>().values(item).execute(client).await.unwrap() }
    })
    .await;
    // Every call after the first collides and reads the existing row back.
    let existing = event(0);
    measure("insert_into on_conflict_do_nothing returning (existing)", || async {
        insert_into::<BenchEvent>()
            .values(&existing)
            .on_conflict_do_nothing(|event| event.id)
            .returning::<BenchEvent>()
            .execute(client)
            .await
            .unwrap()
    })
    .await;
    let ranges = (0..ITERATIONS + 4).map(|_| take(100)).collect::<Vec<_>>();
    let mut ranges = ranges.into_iter();
    measure("insert_many 100 (owned)", || {
        let events = ranges.next().unwrap().map(event).collect::<Vec<_>>();
        async move { insert_many::<BenchEvent>().values(events).execute(client).await.unwrap() }
    })
    .await;
    fixture.clear_events().await.unwrap();
}

async fn print_compile() {
    let sqlite = SqliteAdapter::memory().await.unwrap();
    let postgres = PostgresAdapter::pgbouncer("postgres://postgres:postgres@localhost/postgres").await.unwrap();
    let mysql = MySqlAdapter::new("mysql://root:root@localhost/mysql");
    let compilers: [(&str, &dyn DinocoSqlCompiler); 3] =
        [("sqlite", &sqlite), ("postgres", &postgres), ("mysql", &mysql)];

    println!("\n## SQL compilation ({ITERATIONS} iterations, query construction excluded)\n");
    println!("| compile | allocations/op | bytes/op | peak live bytes |\n|---|---|---|---|");
    for (name, compiler) in compilers {
        compile_row(&format!("{name} find (5 conditions, relation filter)"), complex_find, |query| {
            compiler.compile_find_query(query)
        });
        compile_row(
            &format!("{name} insert 100 rows x 8 fields"),
            || insert_query(100),
            |query| compiler.compile_insert_query(query),
        );
        compile_row(
            &format!("{name} relation occurrence 100 keys"),
            || occurrence_query(100),
            |query| compiler.compile_relation_occurrence_query(query),
        );
        compile_row(
            &format!("{name} aggregate grouped by day (5 conditions, relation filter)"),
            || AggregateQuery {
                source: complex_find(),
                aggregate: AggregateColumn { function: AggregateFunction::SumInteger, field: "age" },
                group: Some(AggregateGroup {
                    key: GroupBy::Date("created_at", DateBucket::Day),
                    order_by: Some(GroupOrder::AggregateDesc),
                    limit: 10,
                    skip: -1,
                }),
            },
            |query| compiler.compile_aggregate_query(query),
        );
        compile_row(
            &format!("{name} lookup 100 keys"),
            || lookup_query(100),
            |query| compiler.compile_lookup_query(query),
        );
        // As includes run it, with the parent keys bound.
        compile_row(
            &format!("{name} relation occurrence 100 integer keys bound"),
            || (occurrence_query(100), (1..=100).map(DinocoValue::Integer).collect::<Vec<_>>()),
            |(query, keys)| compiler.compile_relation_occurrence_query_with_keys(query, keys),
        );
    }
}

fn lookup_query(keys: usize) -> LookupQuery {
    LookupQuery {
        table: "bench_event",
        fields: <BenchEvent as DinocoEntity>::FIELDS,
        key_fields: &["kind", "amount"],
        keys: (0..keys)
            .map(|index| vec![DinocoValue::String(format!("kind-{index}")), DinocoValue::Integer(index as i64)])
            .collect(),
    }
}

fn compile_row<Q, R>(name: &str, mut build: impl FnMut() -> Q, mut compile: impl FnMut(Q) -> R) {
    let queries = (0..ITERATIONS).map(|_| build()).collect::<Vec<_>>();
    let before = snapshot();
    let mut outputs = Vec::with_capacity(queries.len());
    for query in queries {
        outputs.push(compile(query));
    }
    let after = snapshot();
    drop(outputs);
    let query = build();
    let base = reset_peak();
    let output = compile(query);
    let peak = peak().saturating_sub(base);
    drop(output);
    println!(
        "| {name} | {:.1} | {:.0} | {peak} |",
        (after.allocations - before.allocations) as f64 / ITERATIONS as f64,
        (after.bytes - before.bytes) as f64 / ITERATIONS as f64,
    );
}

fn complex_find() -> FindQuery {
    FindQuery {
        fields: &["id", "email", "name", "age", "active", "score", "bio", "created_at"],
        from: "bench_user",
        conditions: vec![
            FindWhere::Gte("age", DinocoValue::Integer(21)),
            FindWhere::Eq("active", DinocoValue::Boolean(true)),
            FindWhere::Or(vec![
                FindWhere::Like("email", DinocoValue::String("%@dinoco.rs".to_string())),
                FindWhere::And(vec![FindWhere::Null("bio"), FindWhere::Lt("score", DinocoValue::Float(100.0))]),
            ]),
            FindWhere::Batch("id", (0..20).map(|index| DinocoValue::String(format!("u-{index:06}"))).collect()),
            FindWhere::Relation(RelationMatch {
                parent_table: "bench_user",
                parent_field: "id",
                child_table: "bench_post",
                child_field: "user_id",
                join: None,
                quantifier: RelationQuantifier::Some,
                conditions: vec![FindWhere::Eq("published", DinocoValue::Boolean(true))],
            }),
        ],
        limit: 50,
        skip: 10,
        order_by: Some(FindOrderBy::Desc("created_at")),
    }
}

fn insert_query(rows: usize) -> InsertQuery {
    InsertQuery {
        table: "bench_event",
        fields: vec!["id", "kind", "payload", "amount", "ratio", "flagged", "note", "created_at"],
        rows: (0..rows)
            .map(|index| {
                vec![
                    DinocoValue::String(format!("e-{index:08}")),
                    DinocoValue::String("signup".to_string()),
                    DinocoValue::String("{\"source\":\"benchmark\"}".to_string()),
                    DinocoValue::Integer(index as i64),
                    DinocoValue::Float(index as f64),
                    DinocoValue::Boolean(index % 2 == 0),
                    DinocoValue::Null,
                    DinocoValue::DateTime(timestamp(index)),
                ]
            })
            .collect(),
        returning: None,
        on_conflict_do_nothing: None,
    }
}

fn occurrence_query(keys: usize) -> RelationOccurrenceQuery {
    RelationOccurrenceQuery {
        query: FindQuery::new(&["id", "user_id", "title", "body", "published"], "bench_post", -1, -1),
        child_field: "user_id",
        key_count: keys,
    }
}

async fn print_bulk_peak(fixture: &Fixture) {
    println!("\n## {} bulk peaks\n", fixture.dialect.name());
    println!("| operation | allocations | bytes | peak live bytes |\n|---|---|---|---|");
    let events = (1_000_000..1_010_000).map(event).collect::<Vec<_>>();
    let before = snapshot();
    let base = reset_peak();
    for chunk in events.chunks(1_000) {
        insert_many::<BenchEvent>().values(chunk.iter().cloned()).execute(&fixture.client).await.unwrap();
    }
    let after = snapshot();
    println!(
        "| insert_many 10x1000 rows (owned) | {} | {} | {} |",
        after.allocations - before.allocations,
        after.bytes - before.bytes,
        peak().saturating_sub(base)
    );
    drop(events);

    let before = snapshot();
    let base = reset_peak();
    let rows = find_many::<BenchEvent>().execute(&fixture.client).await.unwrap();
    let after = snapshot();
    println!(
        "| find_many 10000 rows | {} | {} | {} |",
        after.allocations - before.allocations,
        after.bytes - before.bytes,
        peak().saturating_sub(base)
    );
    assert_eq!(rows.len(), 10_000);
    drop(rows);
    fixture.clear_events().await.unwrap();
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    print_layouts();

    let dialects = std::env::var("DINOCO_BENCH_DIALECTS").unwrap_or_else(|_| "sqlite,postgres,mysql".to_string());
    let dialects = dialects
        .split(',')
        .filter_map(|name| match name.trim() {
            "sqlite" => Some(Dialect::Sqlite),
            "postgres" => Some(Dialect::Postgres),
            "mysql" => Some(Dialect::Mysql),
            _ => None,
        })
        .collect::<Vec<_>>();
    let fixtures = fixtures(&dialects).await;
    if let Some(fixture) = fixtures.first() {
        print_future_sizes(&fixture.client);
    }
    print_compile().await;
    for fixture in &fixtures {
        print_operations(fixture).await;
        print_bulk_peak(fixture).await;
    }

    Ok(())
}
