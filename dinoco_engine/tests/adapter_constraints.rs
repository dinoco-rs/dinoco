mod common;

use dinoco_derives::Rowable;
use dinoco_engine::{
    ConstraintKind, DinocoAdapter, DinocoAdapterHandler, DinocoClient, DinocoError, DinocoResult, MySqlAdapter,
    PostgresAdapter, SqliteAdapter,
};

use crate::common::{mysql_url, postgres_url, sqlite_url, unique_name};

#[derive(Debug, Rowable)]
struct UserRow {
    id: i64,
    email: String,
    age: i64,
}

#[tokio::test]
async fn sqlite_adapter_maps_constraints_and_queries_rows() -> DinocoResult<()> {
    let client = DinocoClient::<SqliteAdapter>::new(sqlite_url("constraints"), vec![]).await?;
    let teams_table = unique_name("teams");
    let users_table = unique_name("users");

    client.primary().reset_database().await?;
    client.primary().execute("PRAGMA foreign_keys = ON", &[]).await?;
    create_sqlite_tables(client.primary(), &teams_table, &users_table).await?;
    exercise_adapter(client.primary(), &teams_table, &users_table, true).await
}

#[tokio::test]
async fn postgres_adapter_maps_constraints_and_queries_rows() -> DinocoResult<()> {
    let client = DinocoClient::<PostgresAdapter>::new(postgres_url(), vec![]).await?;
    let teams_table = unique_name("teams");
    let users_table = unique_name("users");

    client.primary().reset_database().await?;
    create_postgres_tables(client.primary(), &teams_table, &users_table).await?;
    exercise_adapter(client.primary(), &teams_table, &users_table, true).await
}

#[tokio::test]
async fn mysql_adapter_maps_constraints_and_queries_rows() -> DinocoResult<()> {
    let client = DinocoClient::<MySqlAdapter>::new(mysql_url(), vec![]).await?;
    let teams_table = unique_name("teams");
    let users_table = unique_name("users");

    client.primary().reset_database().await?;
    create_mysql_tables(client.primary(), &teams_table, &users_table).await?;
    exercise_adapter(client.primary(), &teams_table, &users_table, false).await
}

async fn exercise_adapter<A: DinocoAdapter>(
    adapter: &A,
    teams_table: &str,
    users_table: &str,
    expect_check: bool,
) -> DinocoResult<()> {
    adapter.execute(&format!("INSERT INTO {teams_table} (id, name) VALUES (1, 'Dinoco')"), &[]).await?;
    adapter
        .execute(
            &format!("INSERT INTO {users_table} (id, email, age, team_id) VALUES (1, 'team@dinoco.dev', 21, 1)"),
            &[],
        )
        .await?;

    let rows =
        adapter.query_as::<UserRow>(&format!("SELECT id, email, age FROM {users_table} ORDER BY id"), &[]).await?;

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, 1);
    assert_eq!(rows[0].email, "team@dinoco.dev");
    assert_eq!(rows[0].age, 21);

    assert_constraint_kind(
        adapter
            .execute(
                &format!("INSERT INTO {users_table} (id, email, age, team_id) VALUES (2, 'team@dinoco.dev', 30, 1)"),
                &[],
            )
            .await
            .expect_err("unique constraint should fail"),
        ConstraintKind::Unique,
    );
    assert_constraint_kind(
        adapter
            .execute(&format!("INSERT INTO {users_table} (id, email, age, team_id) VALUES (3, NULL, 30, 1)"), &[])
            .await
            .expect_err("not null constraint should fail"),
        ConstraintKind::NotNull,
    );
    assert_constraint_kind(
        adapter
            .execute(
                &format!("INSERT INTO {users_table} (id, email, age, team_id) VALUES (4, 'fk@dinoco.dev', 30, 999)"),
                &[],
            )
            .await
            .expect_err("foreign key constraint should fail"),
        ConstraintKind::ForeignKey,
    );

    if expect_check {
        assert_constraint_kind(
            adapter
                .execute(
                    &format!("INSERT INTO {users_table} (id, email, age, team_id) VALUES (5, 'age@dinoco.dev', 10, 1)"),
                    &[],
                )
                .await
                .expect_err("check constraint should fail"),
            ConstraintKind::Check,
        );
    } else {
        let result = adapter
            .execute(
                &format!("INSERT INTO {users_table} (id, email, age, team_id) VALUES (5, 'age@dinoco.dev', 10, 1)"),
                &[],
            )
            .await;

        if let Err(error) = result {
            assert_constraint_kind(error, ConstraintKind::Check);
        }
    }

    Ok(())
}

async fn create_sqlite_tables(adapter: &SqliteAdapter, teams_table: &str, users_table: &str) -> DinocoResult<()> {
    adapter
        .execute(&format!(r#"CREATE TABLE "{teams_table}" ("id" INTEGER PRIMARY KEY, "name" TEXT NOT NULL)"#), &[])
        .await?;
    adapter
        .execute(
            &format!(
                r#"CREATE TABLE "{users_table}" (
                    "id" INTEGER PRIMARY KEY,
                    "email" TEXT NOT NULL UNIQUE,
                    "age" INTEGER NOT NULL CHECK ("age" >= 18),
                    "team_id" INTEGER NOT NULL,
                    FOREIGN KEY ("team_id") REFERENCES "{teams_table}" ("id")
                )"#
            ),
            &[],
        )
        .await?;

    Ok(())
}

async fn create_postgres_tables(adapter: &PostgresAdapter, teams_table: &str, users_table: &str) -> DinocoResult<()> {
    adapter
        .execute(&format!(r#"CREATE TABLE "{teams_table}" ("id" BIGINT PRIMARY KEY, "name" TEXT NOT NULL)"#), &[])
        .await?;
    adapter
        .execute(
            &format!(
                r#"CREATE TABLE "{users_table}" (
                    "id" BIGINT PRIMARY KEY,
                    "email" TEXT NOT NULL UNIQUE,
                    "age" BIGINT NOT NULL CHECK ("age" >= 18),
                    "team_id" BIGINT NOT NULL REFERENCES "{teams_table}" ("id")
                )"#
            ),
            &[],
        )
        .await?;

    Ok(())
}

async fn create_mysql_tables(adapter: &MySqlAdapter, teams_table: &str, users_table: &str) -> DinocoResult<()> {
    adapter
        .execute(&format!("CREATE TABLE `{teams_table}` (`id` BIGINT PRIMARY KEY, `name` VARCHAR(255) NOT NULL)"), &[])
        .await?;
    adapter
        .execute(
            &format!(
                "CREATE TABLE `{users_table}` (\
                    `id` BIGINT PRIMARY KEY,\
                    `email` VARCHAR(255) NOT NULL UNIQUE,\
                    `age` BIGINT NOT NULL CHECK (`age` >= 18),\
                    `team_id` BIGINT NOT NULL,\
                    CONSTRAINT `fk_{users_table}_team_id` FOREIGN KEY (`team_id`) REFERENCES `{teams_table}` (`id`)\
                )"
            ),
            &[],
        )
        .await?;

    Ok(())
}

fn assert_constraint_kind(error: DinocoError, expected: ConstraintKind) {
    match error {
        DinocoError::Constraint(error) => assert_eq!(error.kind, expected),
        other => panic!("expected constraint error, got {other:?}"),
    }
}
