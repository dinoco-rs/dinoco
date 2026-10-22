//! Criterion benchmarks of the Dinoco runtime.
//!
//! - `compile/*`: SQL compilation only (pure Dinoco CPU cost, no I/O).
//! - `<dialect>/*`: end-to-end operations through the public API against an
//!   in-memory SQLite database and, when reachable, the PostgreSQL/MySQL
//!   servers configured by `DINOCO_BENCH_POSTGRES_URL`/`DINOCO_BENCH_MYSQL_URL`.
//! - `<dialect>/raw_driver_*`: the same read through the bare driver, the
//!   floor Dinoco's overhead is measured against.
//!
//! Select dialects with `DINOCO_BENCH_DIALECTS=sqlite,postgres,mysql`.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use criterion::{BatchSize, BenchmarkId, Criterion, criterion_group, criterion_main};
use dinoco::{count, exists, find_first, find_many, insert_into, insert_many, transaction, update};
use dinoco_benchmarks::{BenchEvent, BenchPost, BenchUser, Dialect, Fixture, event, fixtures, raw_users};
use dinoco_engine::{
    AggregateColumn, AggregateFunction, AggregateGroup, AggregateQuery, DateBucket, DinocoEntity, DinocoSqlCompiler,
    DinocoValue, FindOrderBy, FindQuery, FindWhere, GroupBy, GroupOrder, InsertQuery, LookupQuery, MySqlAdapter,
    PostgresAdapter, RelationMatch, RelationOccurrenceQuery, RelationQuantifier, SqliteAdapter, UpdateOperation,
    UpdateQuery, UpdateSet,
};
use tokio::runtime::Runtime;

fn runtime() -> Runtime {
    tokio::runtime::Builder::new_multi_thread().enable_all().build().expect("tokio runtime")
}

fn selected_dialects() -> Vec<Dialect> {
    let selected = std::env::var("DINOCO_BENCH_DIALECTS").unwrap_or_else(|_| "sqlite,postgres,mysql".to_string());
    selected
        .split(',')
        .filter_map(|name| match name.trim() {
            "sqlite" => Some(Dialect::Sqlite),
            "postgres" => Some(Dialect::Postgres),
            "mysql" => Some(Dialect::Mysql),
            _ => None,
        })
        .collect()
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
        order_by: FindOrderBy::Desc("created_at").into(),
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
                    DinocoValue::DateTime(dinoco_benchmarks::timestamp(index)),
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

fn update_query() -> UpdateQuery {
    UpdateQuery {
        table: "bench_user",
        sets: vec![
            UpdateSet {
                field: "name",
                value: DinocoValue::String("renamed".to_string()),
                operation: UpdateOperation::Set,
            },
            UpdateSet { field: "age", value: DinocoValue::Integer(1), operation: UpdateOperation::Increment },
        ],
        conditions: complex_find().conditions,
        returning: Some(&["id", "email", "name", "age", "active", "score", "bio", "created_at"]),
    }
}

fn bench_compile(c: &mut Criterion, rt: &Runtime) {
    let sqlite = rt.block_on(SqliteAdapter::memory()).expect("sqlite");
    let postgres =
        rt.block_on(PostgresAdapter::pgbouncer("postgres://postgres:postgres@localhost/postgres")).expect("pg");
    let mysql = MySqlAdapter::new("mysql://root:root@localhost/mysql");
    let compilers: [(&str, &dyn DinocoSqlCompiler); 3] =
        [("sqlite", &sqlite), ("postgres", &postgres), ("mysql", &mysql)];

    let mut group = c.benchmark_group("compile");
    for (name, compiler) in compilers {
        group.bench_function(BenchmarkId::new("find_complex", name), |b| {
            b.iter_batched(complex_find, |query| compiler.compile_find_query(query), BatchSize::SmallInput)
        });
        group.bench_function(BenchmarkId::new("insert_100x8", name), |b| {
            b.iter_batched(|| insert_query(100), |query| compiler.compile_insert_query(query), BatchSize::SmallInput)
        });
        group.bench_function(BenchmarkId::new("relation_occurrence_100_keys", name), |b| {
            b.iter_batched(
                || occurrence_query(100),
                |query| compiler.compile_relation_occurrence_query(query),
                BatchSize::SmallInput,
            )
        });
        // As includes run it: 100 integer parent keys bound in front.
        group.bench_function(BenchmarkId::new("relation_occurrence_100_int_keys", name), |b| {
            b.iter_batched(
                || (occurrence_query(100), (1..=100).map(DinocoValue::Integer).collect::<Vec<_>>()),
                |(query, keys)| compiler.compile_relation_occurrence_query_with_keys(query, keys),
                BatchSize::SmallInput,
            )
        });
        group.bench_function(BenchmarkId::new("update_returning_complex", name), |b| {
            b.iter_batched(update_query, |query| compiler.compile_update_query(query), BatchSize::SmallInput)
        });
        group.bench_function(BenchmarkId::new("aggregate_grouped_complex", name), |b| {
            b.iter_batched(aggregate_query, |query| compiler.compile_aggregate_query(query), BatchSize::SmallInput)
        });
        group.bench_function(BenchmarkId::new("lookup_100_keys", name), |b| {
            b.iter_batched(|| lookup_query(100), |query| compiler.compile_lookup_query(query), BatchSize::SmallInput)
        });
    }
    group.finish();
}

fn aggregate_query() -> AggregateQuery {
    AggregateQuery {
        source: complex_find(),
        aggregate: AggregateColumn { function: AggregateFunction::SumInteger, field: "age" },
        group: Some(AggregateGroup {
            key: GroupBy::Date("created_at", DateBucket::Day),
            order_by: Some(GroupOrder::AggregateDesc),
            then_by: None,
            limit: 10,
            skip: -1,
        }),
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

fn bench_backend(c: &mut Criterion, rt: &Runtime, fixture: &Fixture) {
    let client = &fixture.client;
    let mut group = c.benchmark_group(fixture.dialect.name());
    group.measurement_time(Duration::from_secs(4)).warm_up_time(Duration::from_secs(1)).sample_size(30);

    for limit in [1usize, 100, 1000] {
        group.bench_function(BenchmarkId::new("find_many", limit), |b| {
            b.to_async(rt).iter(|| async move {
                let rows = find_many::<BenchUser>()
                    .order_by(|user| user.id.asc())
                    .take(limit as i32)
                    .execute(client)
                    .await
                    .expect("find_many");
                assert_eq!(rows.len(), limit);
                rows
            })
        });
        group.bench_function(BenchmarkId::new("raw_driver", limit), |b| {
            b.to_async(rt).iter(|| async move {
                let rows = raw_users(&fixture.raw, limit).await.expect("raw");
                assert_eq!(rows.len(), limit);
                rows
            })
        });
    }

    group.bench_function("find_first_by_id", |b| {
        b.to_async(rt).iter(|| async move {
            find_first::<BenchUser>()
                .where_(|user| user.id.eq("u-000042"))
                .execute(client)
                .await
                .expect("find_first")
                .expect("row")
        })
    });

    group.bench_function("find_where_complex", |b| {
        b.to_async(rt).iter(|| async move {
            find_many::<BenchUser>()
                .where_(|user| user.age.gte(21))
                .where_(|user| user.active.eq(true))
                .where_(|user| user.posts.where_(|post| post.published.eq(true)))
                .order_by(|user| user.created_at.desc())
                .take(50)
                .execute(client)
                .await
                .expect("find_where_complex")
        })
    });

    group.bench_function("includes_has_many_100", |b| {
        b.to_async(rt).iter(|| async move {
            let rows = find_many::<BenchUser>()
                .order_by(|user| user.id.asc())
                .take(100)
                .includes(|user| user.posts())
                .execute(client)
                .await
                .expect("includes");
            assert_eq!(rows[0].posts.len(), dinoco_benchmarks::POSTS_PER_USER);
            rows
        })
    });

    group.bench_function("includes_belongs_to_500", |b| {
        b.to_async(rt).iter(|| async move {
            let rows = find_many::<BenchPost>()
                .order_by(|post| post.id.asc())
                .take(500)
                .includes(|post| post.user())
                .execute(client)
                .await
                .expect("includes");
            assert!(rows[0].user.is_some());
            rows
        })
    });

    group.bench_function("pluck_ids_1000", |b| {
        b.to_async(rt).iter(|| async move {
            find_many::<BenchUser>().take(1000).pluck(|user| user.id).execute(client).await.expect("pluck")
        })
    });

    group.bench_function("pluck_sum", |b| {
        b.to_async(rt).iter(|| async move {
            find_many::<BenchUser>()
                .where_(|user| user.active.eq(true))
                .pluck(|user| user.age.sum())
                .execute(client)
                .await
                .expect("pluck sum")
        })
    });

    group.bench_function("pluck_sum_group_by_day", |b| {
        b.to_async(rt).iter(|| async move {
            find_many::<BenchUser>()
                .pluck(|user| user.age.sum())
                .group_by(|user| user.created_at.day())
                .execute(client)
                .await
                .expect("pluck group")
        })
    });

    group.bench_function("pluck_sum_group_by_day_in_time_zone", |b| {
        b.to_async(rt).iter(|| async move {
            find_many::<BenchUser>()
                .pluck(|user| user.age.sum())
                .group_by(|user| user.created_at.day().with_timezone(&dinoco_engine::chrono_tz::America::Sao_Paulo))
                .execute(client)
                .await
                .expect("pluck group in zone")
        })
    });

    group.bench_function("find_many_two_order_columns", |b| {
        b.to_async(rt).iter(|| async move {
            find_many::<BenchUser>()
                .order_by(|user| user.created_at.desc())
                .order_by(|user| user.id.asc())
                .take(100)
                .execute(client)
                .await
                .expect("find many")
        })
    });

    group.bench_function("count", |b| {
        b.to_async(rt).iter(|| async move {
            count::<BenchUser>().where_(|user| user.active.eq(true)).execute(client).await.expect("count")
        })
    });

    group.bench_function("count_includes_has_many", |b| {
        b.to_async(rt).iter(|| async move {
            count::<BenchUser>()
                .where_(|user| user.active.eq(true))
                .includes(|user| user.posts().where_(|post| post.published.eq(true)))
                .execute(client)
                .await
                .expect("count includes")
        })
    });

    group.bench_function("exists", |b| {
        b.to_async(rt).iter(|| async move {
            exists::<BenchUser>().where_(|user| user.id.eq("u-000042")).execute(client).await.expect("exists")
        })
    });

    group.bench_function("update_by_id", |b| {
        b.to_async(rt).iter(|| async move {
            update::<BenchUser>()
                .where_(|user| user.id.eq("u-000007"))
                .update(|user| user.score.increment(1.0))
                .execute(client)
                .await
                .expect("update")
        })
    });

    let next_event = AtomicUsize::new(0);
    let next_event = &next_event;
    for rows in [1usize, 100] {
        group.bench_function(BenchmarkId::new("insert_many", rows), |b| {
            b.to_async(rt).iter_custom(|iters| async move {
                let mut elapsed = Duration::ZERO;
                for _ in 0..iters {
                    let start_index = next_event.fetch_add(rows, Ordering::Relaxed);
                    let events = (start_index..start_index + rows).map(event).collect::<Vec<_>>();
                    let start = Instant::now();
                    insert_many::<BenchEvent>().values(events).execute(client).await.expect("insert_many");
                    elapsed += start.elapsed();
                }
                fixture.clear_events().await.expect("clear events");
                elapsed
            })
        });
    }

    group.bench_function("insert_one_by_reference", |b| {
        b.to_async(rt).iter_custom(|iters| async move {
            let mut elapsed = Duration::ZERO;
            for _ in 0..iters {
                let item = event(next_event.fetch_add(1, Ordering::Relaxed));
                let start = Instant::now();
                insert_into::<BenchEvent>().values(&item).execute(client).await.expect("insert");
                elapsed += start.elapsed();
            }
            fixture.clear_events().await.expect("clear events");
            elapsed
        })
    });

    // Every iteration collides with the same row and reads it back.
    group.bench_function("insert_on_conflict_returning_existing", |b| {
        b.to_async(rt).iter_custom(|iters| async move {
            let item = event(next_event.fetch_add(1, Ordering::Relaxed));
            insert_into::<BenchEvent>().values(&item).execute(client).await.expect("seed conflict");
            let start = Instant::now();
            for _ in 0..iters {
                insert_into::<BenchEvent>()
                    .values(&item)
                    .on_conflict_do_nothing(|event| event.id)
                    .returning::<BenchEvent>()
                    .execute(client)
                    .await
                    .expect("insert on conflict");
            }
            let elapsed = start.elapsed();
            fixture.clear_events().await.expect("clear events");
            elapsed
        })
    });

    group.bench_function("transaction_read_write", |b| {
        b.to_async(rt).iter(|| async move {
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
            .expect("transaction")
        })
    });

    group.bench_function("transaction_count_exists", |b| {
        b.to_async(rt).iter(|| async move {
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
            .expect("transaction")
        })
    });

    group.finish();
}

fn benches(c: &mut Criterion) {
    let rt = runtime();
    // Pools are dropped (and SQLite connections closed) through the runtime.
    let _context = rt.enter();
    bench_compile(c, &rt);

    let fixtures = rt.block_on(fixtures(&selected_dialects()));
    for fixture in &fixtures {
        bench_backend(c, &rt, fixture);
    }
}

criterion_group!(runtime_benches, benches);
criterion_main!(runtime_benches);
