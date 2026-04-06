mod common;

use dinoco_derives::Rowable;
use dinoco_engine::{
    DinocoAdapter, DinocoAdapterHandler, DinocoClient, MySqlAdapter, MySqlDialect, OrderDirection, PostgresAdapter,
    PostgresDialect, QueryBuilder, SelectStatement, SqliteAdapter, SqliteDialect,
};

use crate::common::{mysql_url, postgres_url, sqlite_url, unique_name};

#[derive(Debug, Rowable)]
struct RankedEvent {
    id: i64,
    group_id: i64,
    score: i64,
    row_num: i64,
}

#[tokio::test]
async fn sqlite_window_function_query_runs_with_partitioned_select() {
    let client =
        DinocoClient::<SqliteAdapter>::new(sqlite_url("window"), vec![]).await.expect("sqlite client should connect");
    let table = unique_name("events");

    client.primary().reset_database().await.expect("sqlite database should reset");
    create_events_table(client.primary(), &table, DatabaseKind::Sqlite).await;
    assert_top_rank_per_group(client.primary(), SqliteDialect, &table).await;
}

#[tokio::test]
async fn postgres_window_function_query_runs_with_partitioned_select() {
    let client =
        DinocoClient::<PostgresAdapter>::new(postgres_url(), vec![]).await.expect("postgres client should connect");
    let table = unique_name("events");

    client.primary().reset_database().await.expect("postgres database should reset");
    create_events_table(client.primary(), &table, DatabaseKind::Postgres).await;
    assert_top_rank_per_group(client.primary(), PostgresDialect, &table).await;
}

#[tokio::test]
async fn mysql_window_function_query_runs_with_partitioned_select() {
    let client = DinocoClient::<MySqlAdapter>::new(mysql_url(), vec![]).await.expect("mysql client should connect");
    let table = unique_name("events");

    client.primary().reset_database().await.expect("mysql database should reset");
    create_events_table(client.primary(), &table, DatabaseKind::MySql).await;
    assert_top_rank_per_group(client.primary(), MySqlDialect, &table).await;
}

async fn assert_top_rank_per_group<A, D>(adapter: &A, dialect: D, table: &str)
where
    A: DinocoAdapter,
    D: QueryBuilder,
{
    let statement = SelectStatement::new()
        .select(&["id", "group_id", "score"])
        .from(table)
        .order_by("score", OrderDirection::Desc)
        .limit(1);
    let (sql, params) = dialect.build_partitioned_select(&statement, "group_id", "row_num");
    let rows = adapter.query_as::<RankedEvent>(&sql, &params).await.expect("partitioned query should execute");

    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].group_id, 1);
    assert_eq!(rows[0].id, 2);
    assert_eq!(rows[0].score, 20);
    assert_eq!(rows[0].row_num, 1);
    assert_eq!(rows[1].group_id, 2);
    assert_eq!(rows[1].id, 4);
    assert_eq!(rows[1].score, 15);
    assert_eq!(rows[1].row_num, 1);
}

async fn create_events_table<A: DinocoAdapter>(adapter: &A, table: &str, database: DatabaseKind) {
    let create_table = match database {
        DatabaseKind::Sqlite => {
            format!(
                r#"CREATE TABLE "{table}" ("id" INTEGER PRIMARY KEY, "group_id" INTEGER NOT NULL, "score" INTEGER NOT NULL)"#
            )
        }
        DatabaseKind::Postgres => {
            format!(
                r#"CREATE TABLE "{table}" ("id" BIGINT PRIMARY KEY, "group_id" BIGINT NOT NULL, "score" BIGINT NOT NULL)"#
            )
        }
        DatabaseKind::MySql => {
            format!(
                "CREATE TABLE `{table}` (`id` BIGINT PRIMARY KEY, `group_id` BIGINT NOT NULL, `score` BIGINT NOT NULL)"
            )
        }
    };

    adapter.execute(&create_table, &[]).await.expect("events table should be created");

    for insert in render_event_inserts(table, database) {
        adapter.execute(&insert, &[]).await.expect("event row should be inserted");
    }
}

fn render_event_inserts(table: &str, database: DatabaseKind) -> Vec<String> {
    let table = match database {
        DatabaseKind::Sqlite | DatabaseKind::Postgres => format!(r#""{table}""#),
        DatabaseKind::MySql => format!("`{table}`"),
    };

    vec![
        format!("INSERT INTO {table} (id, group_id, score) VALUES (1, 1, 10)"),
        format!("INSERT INTO {table} (id, group_id, score) VALUES (2, 1, 20)"),
        format!("INSERT INTO {table} (id, group_id, score) VALUES (3, 2, 5)"),
        format!("INSERT INTO {table} (id, group_id, score) VALUES (4, 2, 15)"),
    ]
}

#[derive(Clone, Copy)]
enum DatabaseKind {
    Sqlite,
    Postgres,
    MySql,
}
