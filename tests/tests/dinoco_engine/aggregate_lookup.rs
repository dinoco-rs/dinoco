//! The SQL of aggregate queries (`find_many(...).pluck(field.sum())` and
//! `.group_by(...)`), of inserts with `on_conflict_do_nothing`, and of the
//! lookup that reads their rows back, in every dialect.

use dinoco_engine::chrono_tz::America::Sao_Paulo;
use dinoco_engine::{
    AggregateColumn, AggregateFunction, AggregateGroup, AggregateQuery, DateBucket, DinocoAdapter, DinocoSqlCompiler,
    DinocoValue, FindOrderBy, FindQuery, FindWhere, GroupBy, GroupOrder, InsertQuery, LookupQuery, MySqlAdapter,
    PostgresAdapter, SqliteAdapter,
};

struct Compilers {
    sqlite: SqliteAdapter,
    postgres: PostgresAdapter,
    mysql: MySqlAdapter,
}

impl Compilers {
    async fn new() -> anyhow::Result<Self> {
        Ok(Self {
            sqlite: SqliteAdapter::new(":memory:".to_string()).await.map_err(anyhow::Error::msg)?,
            postgres: PostgresAdapter::pgbouncer("postgres://postgres:postgres@localhost/postgres").await?,
            mysql: MySqlAdapter::new("mysql://root:root@localhost/mysql"),
        })
    }

    /// `(sqlite, postgres, mysql)`.
    fn each(&self) -> [&dyn DinocoSqlCompiler; 3] {
        [&self.sqlite, &self.postgres, &self.mysql]
    }
}

fn string(value: &str) -> DinocoValue {
    DinocoValue::String(value.to_string())
}

fn source(conditions: Vec<FindWhere>) -> FindQuery {
    let mut query = FindQuery::new(&[], "ag_order", -1, -1);
    query.conditions = conditions;
    query
}

fn aggregate(function: AggregateFunction, field: &'static str) -> AggregateColumn {
    AggregateColumn { function, field }
}

#[tokio::test]
async fn single_aggregates_cast_to_one_type_in_every_dialect() -> anyhow::Result<()> {
    let compilers = Compilers::new().await?;
    let cases = [
        (
            AggregateFunction::SumInteger,
            [
                "COALESCE(SUM(amount), 0)",
                "CAST(COALESCE(SUM(amount), 0) AS BIGINT)",
                "CAST(COALESCE(SUM(amount), 0) AS SIGNED)",
            ],
        ),
        (
            AggregateFunction::SumFloat,
            [
                "TOTAL(amount)",
                "CAST(COALESCE(SUM(amount), 0) AS DOUBLE PRECISION)",
                "CAST(COALESCE(SUM(amount), 0) AS DOUBLE)",
            ],
        ),
        (
            AggregateFunction::Avg,
            ["AVG(amount)", "CAST(AVG(amount) AS DOUBLE PRECISION)", "CAST(AVG(amount) AS DOUBLE)"],
        ),
        (AggregateFunction::Min, ["MIN(amount)", "MIN(amount)", "MIN(amount)"]),
        (AggregateFunction::Max, ["MAX(amount)", "MAX(amount)", "MAX(amount)"]),
        (AggregateFunction::Count, ["COUNT(amount)", "COUNT(amount)", "COUNT(amount)"]),
    ];

    for (function, expressions) in cases {
        for ((compiler, expression), placeholder) in compilers.each().into_iter().zip(expressions).zip(["?", "$1", "?"])
        {
            // `order_by` alone picks no window, so it is left out.
            let mut rows = source(vec![FindWhere::Eq("status", string("paid"))]);
            rows.order_by = FindOrderBy::Desc("amount").into();
            let (sql, params) = compiler.compile_aggregate_query(AggregateQuery {
                source: rows,
                aggregate: aggregate(function, "amount"),
                group: None,
            });

            assert_eq!(sql, format!("SELECT {expression} FROM ag_order WHERE status = {placeholder}"));
            assert_eq!(params.len(), 1);
        }
    }

    Ok(())
}

#[tokio::test]
async fn take_and_skip_aggregate_a_window_of_the_rows() -> anyhow::Result<()> {
    let compilers = Compilers::new().await?;
    let expected = [
        "SELECT COUNT(id) FROM (SELECT id FROM ag_order WHERE status = ? ORDER BY created_at DESC LIMIT ? OFFSET ?) \
         AS __dinoco_rows",
        "SELECT COUNT(id) FROM (SELECT id FROM ag_order WHERE status = $1 ORDER BY created_at DESC LIMIT $2 OFFSET $3) \
         AS __dinoco_rows",
        "SELECT COUNT(id) FROM (SELECT id FROM ag_order WHERE status = ? ORDER BY created_at DESC LIMIT ? OFFSET ?) \
         AS __dinoco_rows",
    ];

    for (compiler, expected) in compilers.each().into_iter().zip(expected) {
        let mut rows = source(vec![FindWhere::Eq("status", string("paid"))]);
        rows.order_by = FindOrderBy::Desc("created_at").into();
        rows.limit = 3;
        rows.skip = 1;
        let (sql, params) = compiler.compile_aggregate_query(AggregateQuery {
            source: rows,
            aggregate: aggregate(AggregateFunction::Count, "id"),
            group: None,
        });

        assert_eq!(sql, expected);
        assert_eq!(params.len(), 3);
        assert!(matches!(params[1..], [DinocoValue::Integer(3), DinocoValue::Integer(1)]), "{params:?}");
    }

    Ok(())
}

#[tokio::test]
async fn groups_select_the_key_and_the_aggregate_ordered_and_paged() -> anyhow::Result<()> {
    let compilers = Compilers::new().await?;
    let group = |key, order_by| AggregateGroup { key, order_by, then_by: None, limit: 2, skip: 1 };
    let (sql, params) = compilers.postgres.compile_aggregate_query(AggregateQuery {
        source: source(vec![FindWhere::Gt("amount", DinocoValue::Integer(10))]),
        aggregate: aggregate(AggregateFunction::SumInteger, "amount"),
        group: Some(group(GroupBy::Date("created_at", DateBucket::Day), None)),
    });
    assert_eq!(
        sql,
        "SELECT CAST(created_at AS DATE) AS __dinoco_group_key, CAST(COALESCE(SUM(amount), 0) AS BIGINT) AS \
         __dinoco_aggregate FROM ag_order WHERE amount > $1 GROUP BY __dinoco_group_key ORDER BY __dinoco_group_key \
         ASC LIMIT $2 OFFSET $3"
    );
    assert!(matches!(params[..], [DinocoValue::Integer(10), DinocoValue::Integer(2), DinocoValue::Integer(1)]));

    let orders = [
        (None, "ORDER BY __dinoco_group_key ASC"),
        (Some(GroupOrder::KeyAsc), "ORDER BY __dinoco_group_key ASC"),
        (Some(GroupOrder::KeyDesc), "ORDER BY __dinoco_group_key DESC"),
        (Some(GroupOrder::AggregateAsc), "ORDER BY __dinoco_aggregate ASC, __dinoco_group_key ASC"),
        (Some(GroupOrder::AggregateDesc), "ORDER BY __dinoco_aggregate DESC, __dinoco_group_key ASC"),
    ];
    for (compiler, placeholders) in compilers.each().into_iter().zip([["?", "?"], ["$1", "$2"], ["?", "?"]]) {
        for (order_by, order_sql) in orders {
            let (sql, _) = compiler.compile_aggregate_query(AggregateQuery {
                source: source(Vec::new()),
                aggregate: aggregate(AggregateFunction::Count, "id"),
                group: Some(group(GroupBy::Field("status"), order_by)),
            });

            assert_eq!(
                sql,
                format!(
                    "SELECT status AS __dinoco_group_key, COUNT(id) AS __dinoco_aggregate FROM ag_order GROUP BY \
                     __dinoco_group_key {order_sql} LIMIT {} OFFSET {}",
                    placeholders[0], placeholders[1]
                )
            );
        }
    }

    Ok(())
}

#[tokio::test]
async fn date_buckets_truncate_to_the_first_day_of_the_period() -> anyhow::Result<()> {
    let compilers = Compilers::new().await?;
    let cases = [
        (DateBucket::Day, ["date(created_at)", "CAST(created_at AS DATE)", "DATE(created_at)"]),
        (
            DateBucket::Week,
            [
                "date(created_at, 'weekday 0', '-6 days')",
                "CAST(date_trunc('week', created_at) AS DATE)",
                "DATE_SUB(DATE(created_at), INTERVAL WEEKDAY(created_at) DAY)",
            ],
        ),
        (
            DateBucket::Month,
            [
                "date(created_at, 'start of month')",
                "CAST(date_trunc('month', created_at) AS DATE)",
                "DATE_SUB(DATE(created_at), INTERVAL DAYOFMONTH(created_at) - 1 DAY)",
            ],
        ),
        (
            DateBucket::Year,
            [
                "date(created_at, 'start of year')",
                "CAST(date_trunc('year', created_at) AS DATE)",
                "MAKEDATE(YEAR(created_at), 1)",
            ],
        ),
        (
            DateBucket::Hour,
            [
                "strftime('%Y-%m-%dT%H:00:00+00:00', created_at)",
                "date_trunc('hour', created_at)",
                "CAST(DATE_FORMAT(created_at, '%Y-%m-%d %H:00:00') AS DATETIME)",
            ],
        ),
        (
            DateBucket::Minute,
            [
                "strftime('%Y-%m-%dT%H:%M:00+00:00', created_at)",
                "date_trunc('minute', created_at)",
                "CAST(DATE_FORMAT(created_at, '%Y-%m-%d %H:%i:00') AS DATETIME)",
            ],
        ),
    ];

    for (bucket, expressions) in cases {
        for (compiler, expression) in compilers.each().into_iter().zip(expressions) {
            let (sql, params) = compiler.compile_aggregate_query(AggregateQuery {
                source: source(Vec::new()),
                aggregate: aggregate(AggregateFunction::Count, "id"),
                group: Some(AggregateGroup {
                    key: GroupBy::Date("created_at", bucket),
                    order_by: None,
                    then_by: None,
                    limit: -1,
                    skip: -1,
                }),
            });

            assert_eq!(
                sql,
                format!(
                    "SELECT {expression} AS __dinoco_group_key, COUNT(id) AS __dinoco_aggregate FROM ag_order \
                     GROUP BY __dinoco_group_key ORDER BY __dinoco_group_key ASC"
                )
            );
            assert!(params.is_empty());
        }
    }

    Ok(())
}

#[tokio::test]
async fn zoned_date_buckets_read_the_column_as_local_time_first() -> anyhow::Result<()> {
    let compilers = Compilers::new().await?;
    // The UTC expressions of `date_buckets_truncate_to_the_first_day_of_the_period`,
    // with the column read as São Paulo wall-clock time.
    let local = [
        "dinoco_local_seconds(created_at, 'America/Sao_Paulo'), 'unixepoch'",
        "(created_at AT TIME ZONE 'UTC' AT TIME ZONE 'America/Sao_Paulo')",
        "CONVERT_TZ(created_at, '+00:00', 'America/Sao_Paulo')",
    ];
    let cases = [
        (DateBucket::Day, ["date(X)", "CAST(X AS DATE)", "DATE(X)"]),
        (
            DateBucket::Week,
            [
                "date(X, 'weekday 0', '-6 days')",
                "CAST(date_trunc('week', X) AS DATE)",
                "DATE_SUB(DATE(X), INTERVAL WEEKDAY(X) DAY)",
            ],
        ),
        (
            DateBucket::Month,
            [
                "date(X, 'start of month')",
                "CAST(date_trunc('month', X) AS DATE)",
                "DATE_SUB(DATE(X), INTERVAL DAYOFMONTH(X) - 1 DAY)",
            ],
        ),
        (DateBucket::Year, ["date(X, 'start of year')", "CAST(date_trunc('year', X) AS DATE)", "MAKEDATE(YEAR(X), 1)"]),
    ];

    for (bucket, expressions) in cases {
        for ((compiler, expression), local) in compilers.each().into_iter().zip(expressions).zip(local) {
            let (sql, params) = compiler.compile_aggregate_query(AggregateQuery {
                source: source(Vec::new()),
                aggregate: aggregate(AggregateFunction::Count, "id"),
                group: Some(AggregateGroup {
                    key: GroupBy::DateIn("created_at", bucket, Sao_Paulo),
                    order_by: None,
                    then_by: None,
                    limit: -1,
                    skip: -1,
                }),
            });

            assert_eq!(
                sql,
                format!(
                    "SELECT {} AS __dinoco_group_key, COUNT(id) AS __dinoco_aggregate FROM ag_order \
                     GROUP BY __dinoco_group_key ORDER BY __dinoco_group_key ASC",
                    expression.replace('X', local)
                )
            );
            assert!(params.is_empty(), "the zone is a literal, not a parameter");
        }
    }

    Ok(())
}

#[tokio::test]
async fn a_second_group_order_breaks_aggregate_ties_by_the_key() -> anyhow::Result<()> {
    let compilers = Compilers::new().await?;
    let order_sql = |order_by, then_by| {
        let (sql, _) = compilers.sqlite.compile_aggregate_query(AggregateQuery {
            source: source(Vec::new()),
            aggregate: aggregate(AggregateFunction::Count, "id"),
            group: Some(AggregateGroup { key: GroupBy::Field("status"), order_by, then_by, limit: -1, skip: -1 }),
        });
        sql.split_once(" ORDER BY ").map(|(_, order)| order.to_string()).unwrap_or_default()
    };

    assert_eq!(order_sql(Some(GroupOrder::AggregateDesc), None), "__dinoco_aggregate DESC, __dinoco_group_key ASC");
    assert_eq!(
        order_sql(Some(GroupOrder::AggregateDesc), Some(GroupOrder::KeyDesc)),
        "__dinoco_aggregate DESC, __dinoco_group_key DESC"
    );
    assert_eq!(
        order_sql(Some(GroupOrder::AggregateAsc), Some(GroupOrder::KeyDesc)),
        "__dinoco_aggregate ASC, __dinoco_group_key DESC"
    );
    assert_eq!(
        order_sql(Some(GroupOrder::AggregateAsc), Some(GroupOrder::KeyAsc)),
        "__dinoco_aggregate ASC, __dinoco_group_key ASC"
    );
    // Keys are unique: an order after the key has nothing to break.
    assert_eq!(order_sql(Some(GroupOrder::KeyDesc), Some(GroupOrder::AggregateAsc)), "__dinoco_group_key DESC");

    Ok(())
}

#[tokio::test]
async fn windowed_groups_read_the_key_column_once() -> anyhow::Result<()> {
    let compilers = Compilers::new().await?;
    let window = |key| {
        let mut rows = source(Vec::new());
        rows.limit = 10;
        AggregateQuery {
            source: rows,
            aggregate: aggregate(AggregateFunction::Max, "amount"),
            group: Some(AggregateGroup { key, order_by: None, then_by: None, limit: -1, skip: -1 }),
        }
    };

    let (sql, _) = compilers.sqlite.compile_aggregate_query(window(GroupBy::Date("created_at", DateBucket::Day)));
    assert_eq!(
        sql,
        "SELECT date(created_at) AS __dinoco_group_key, MAX(amount) AS __dinoco_aggregate FROM (SELECT amount, \
         created_at FROM ag_order LIMIT ?) AS __dinoco_rows GROUP BY __dinoco_group_key ORDER BY __dinoco_group_key ASC"
    );

    let (sql, _) = compilers.sqlite.compile_aggregate_query(window(GroupBy::Field("amount")));
    assert!(sql.contains("FROM (SELECT amount FROM ag_order LIMIT ?) AS __dinoco_rows"), "{sql}");

    Ok(())
}

#[tokio::test]
async fn aggregates_quote_reserved_identifiers() -> anyhow::Result<()> {
    let compilers = Compilers::new().await?;
    let mut rows = FindQuery::new(&[], "order", -1, -1);
    rows.conditions = vec![FindWhere::Eq("user", string("ana"))];
    let query = AggregateQuery {
        source: rows,
        aggregate: aggregate(AggregateFunction::Max, "key"),
        group: Some(AggregateGroup {
            key: GroupBy::Date("range", DateBucket::Day),
            order_by: None,
            then_by: None,
            limit: -1,
            skip: -1,
        }),
    };

    let expected = [
        "SELECT date(\"range\") AS __dinoco_group_key, MAX(\"key\") AS __dinoco_aggregate FROM \"order\" WHERE \
         \"user\" = ? GROUP BY __dinoco_group_key ORDER BY __dinoco_group_key ASC",
        "SELECT CAST(\"range\" AS DATE) AS __dinoco_group_key, MAX(\"key\") AS __dinoco_aggregate FROM \"order\" \
         WHERE \"user\" = $1 GROUP BY __dinoco_group_key ORDER BY __dinoco_group_key ASC",
        "SELECT DATE(`range`) AS __dinoco_group_key, MAX(`key`) AS __dinoco_aggregate FROM `order` WHERE `user` = ? \
         GROUP BY __dinoco_group_key ORDER BY __dinoco_group_key ASC",
    ];
    for (compiler, expected) in compilers.each().into_iter().zip(expected) {
        assert_eq!(compiler.compile_aggregate_query(query.clone()).0, expected);
    }

    Ok(())
}

fn insert(on_conflict_do_nothing: &'static [&'static str], returning: Option<&'static [&'static str]>) -> InsertQuery {
    InsertQuery {
        table: "ic_member",
        fields: vec!["id", "tenant", "handle"],
        rows: vec![
            vec![string("m1"), string("acme"), string("ana")],
            vec![string("m2"), string("acme"), string("bia")],
        ],
        returning,
        on_conflict_do_nothing: Some(on_conflict_do_nothing),
    }
}

#[tokio::test]
async fn inserts_skip_conflicts_on_the_target_and_mysql_on_any_unique_index() -> anyhow::Result<()> {
    let compilers = Compilers::new().await?;
    let values = ["(?, ?, ?), (?, ?, ?)", "($1, $2, $3), ($4, $5, $6)", "(?, ?, ?), (?, ?, ?)"];
    let single = [
        " ON CONFLICT (handle) DO NOTHING",
        " ON CONFLICT (handle) DO NOTHING",
        " ON DUPLICATE KEY UPDATE handle = handle",
    ];
    let composite = [
        " ON CONFLICT (tenant, handle) DO NOTHING",
        " ON CONFLICT (tenant, handle) DO NOTHING",
        " ON DUPLICATE KEY UPDATE tenant = tenant",
    ];

    for (index, compiler) in compilers.each().into_iter().enumerate() {
        let prefix = format!("INSERT INTO ic_member (id, tenant, handle) VALUES {}", values[index]);

        let (sql, params) = compiler.compile_insert_query(insert(&["handle"], None));
        assert_eq!(sql, format!("{prefix}{}", single[index]));
        assert_eq!(params.len(), 6);

        let (sql, _) = compiler.compile_insert_query(insert(&["tenant", "handle"], None));
        assert_eq!(sql, format!("{prefix}{}", composite[index]));
    }

    // `RETURNING` follows the conflict clause.
    let (sql, _) = compilers.postgres.compile_insert_query(insert(&["handle"], Some(&["id"])));
    assert!(sql.ends_with(" ON CONFLICT (handle) DO NOTHING RETURNING id"), "{sql}");

    // Reserved names are quoted in the clause too.
    let mut reserved = insert(&["user"], None);
    reserved.fields = vec!["id", "user", "group"];
    assert!(compilers.sqlite.compile_insert_query(reserved.clone()).0.ends_with(" ON CONFLICT (\"user\") DO NOTHING"));
    assert!(compilers.mysql.compile_insert_query(reserved).0.ends_with(" ON DUPLICATE KEY UPDATE `user` = `user`"));

    Ok(())
}

#[tokio::test]
async fn lookups_join_the_keys_and_keep_their_order() -> anyhow::Result<()> {
    let compilers = Compilers::new().await?;
    let query = || LookupQuery {
        table: "ic_member",
        fields: &["id", "visits"],
        key_fields: &["tenant", "visits"],
        keys: vec![vec![string("acme"), DinocoValue::Integer(3)], vec![string("globex"), DinocoValue::Integer(1)]],
    };
    let select = "SELECT ic_member.id, ic_member.visits FROM __dinoco_lookup_keys INNER JOIN ic_member ON \
                  ic_member.tenant = __dinoco_lookup_keys.__dinoco_lookup_key_0 AND ic_member.visits = \
                  __dinoco_lookup_keys.__dinoco_lookup_key_1 ORDER BY __dinoco_lookup_keys.__dinoco_lookup_ordinal";

    let (sql, params) = compilers.sqlite.compile_lookup_query(query());
    assert_eq!(
        sql,
        format!(
            "WITH __dinoco_lookup_keys(__dinoco_lookup_ordinal, __dinoco_lookup_key_0, __dinoco_lookup_key_1) AS \
             (VALUES (0, ?, ?), (1, ?, ?)) {select}"
        )
    );
    assert_eq!(params.len(), 4);

    // PostgreSQL types every key by the value bound to it.
    let (sql, params) = compilers.postgres.compile_lookup_query(query());
    assert_eq!(
        sql,
        format!(
            "WITH __dinoco_lookup_keys(__dinoco_lookup_ordinal, __dinoco_lookup_key_0, __dinoco_lookup_key_1) AS \
             (VALUES (0, $1::text, $2::bigint), (1, $3::text, $4::bigint)) {select}"
        )
    );
    assert!(matches!(params[1], DinocoValue::Integer(3)));

    let (sql, params) = compilers.mysql.compile_lookup_query(query());
    assert_eq!(
        sql,
        format!(
            "WITH __dinoco_lookup_keys AS (SELECT 0 AS __dinoco_lookup_ordinal, ? AS __dinoco_lookup_key_0, ? AS \
             __dinoco_lookup_key_1 UNION ALL SELECT 1, ?, ?) {select}"
        )
    );
    assert_eq!(params.len(), 4);

    Ok(())
}

#[tokio::test]
async fn skip_without_take_gets_a_limit_where_offset_needs_one() -> anyhow::Result<()> {
    let compilers = Compilers::new().await?;
    let expected = [
        "SELECT id FROM ag_order ORDER BY id ASC LIMIT -1 OFFSET ?",
        "SELECT id FROM ag_order ORDER BY id ASC OFFSET $1",
        "SELECT id FROM ag_order ORDER BY id ASC LIMIT 18446744073709551615 OFFSET ?",
    ];

    for (compiler, expected) in compilers.each().into_iter().zip(expected) {
        let mut query = FindQuery::new(&["id"], "ag_order", -1, 2);
        query.order_by = FindOrderBy::Asc("id").into();
        let (sql, params) = compiler.compile_find_query(query);

        assert_eq!(sql, expected);
        assert!(matches!(params[..], [DinocoValue::Integer(2)]), "{params:?}");

        // Groups page the same way.
        let (sql, _) = compiler.compile_aggregate_query(AggregateQuery {
            source: source(Vec::new()),
            aggregate: aggregate(AggregateFunction::Count, "id"),
            group: Some(AggregateGroup {
                key: GroupBy::Field("status"),
                order_by: None,
                then_by: None,
                limit: -1,
                skip: 1,
            }),
        });
        assert!(sql.ends_with(expected.rsplit_once(" ASC ").map(|(_, page)| page).unwrap_or_default()), "{sql}");
    }

    Ok(())
}
