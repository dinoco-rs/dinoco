use dinoco_engine::rusqlite::{self, ffi};
use dinoco_engine::{
    ConstraintDetails, DatabaseConstraintError, DatabaseError, DinocoAdapter, PostgresAdapter, mysql_async,
};

const POSTGRES_URL: &str = "postgres://postgres:postgres@localhost:5432/postgres";

fn sqlite_details(extended_code: i32, message: &str) -> ConstraintDetails {
    let error = rusqlite::Error::SqliteFailure(ffi::Error::new(extended_code), Some(message.to_string()));
    DatabaseError::new(error.into()).constraint_details().clone()
}

fn mysql_details(code: u16, message: &str) -> ConstraintDetails {
    let error = mysql_async::Error::Server(mysql_async::ServerError {
        code,
        message: message.to_string(),
        state: "23000".to_string(),
    });
    DatabaseError::new(error.into()).constraint_details().clone()
}

#[test]
fn sqlite_messages_expose_table_and_columns() {
    assert_eq!(
        sqlite_details(ffi::SQLITE_CONSTRAINT_UNIQUE, "UNIQUE constraint failed: account.email, account.tenant_id"),
        ConstraintDetails {
            table: Some("account".to_string()),
            constraint: None,
            columns: vec!["email".to_string(), "tenant_id".to_string()],
        }
    );
    assert_eq!(
        sqlite_details(ffi::SQLITE_CONSTRAINT_CHECK, "CHECK constraint failed: positive_balance").constraint.as_deref(),
        Some("positive_balance")
    );
    assert_eq!(
        sqlite_details(ffi::SQLITE_CONSTRAINT_FOREIGNKEY, "FOREIGN KEY constraint failed"),
        ConstraintDetails::default()
    );
}

#[test]
fn mysql_messages_expose_constraint_and_columns() {
    let unique = mysql_details(1062, "Duplicate entry 'a@dinoco.rs' for key 'account.account_email_key'");
    assert_eq!(unique.table.as_deref(), Some("account"));
    assert_eq!(unique.constraint.as_deref(), Some("account_email_key"));

    let not_null = mysql_details(1048, "Column 'email' cannot be null");
    assert_eq!(not_null.columns, vec!["email"]);

    let foreign_key = mysql_details(
        1452,
        "Cannot add or update a child row: a foreign key constraint fails (`app`.`session`, CONSTRAINT `session_account_fk` FOREIGN KEY (`account_id`) REFERENCES `account` (`id`))",
    );
    assert_eq!(foreign_key.table.as_deref(), Some("session"));
    assert_eq!(foreign_key.constraint.as_deref(), Some("session_account_fk"));
    assert_eq!(foreign_key.columns, vec!["account_id"]);

    let check = mysql_details(3819, "Check constraint 'positive_balance' is violated.");
    assert_eq!(check.constraint.as_deref(), Some("positive_balance"));
}

// PostgreSQL leaves the column of a unique violation empty and names it only in
// the detail (`Key (tenant_id, "Email")=(...) already exists.`), and its
// driver error cannot be built by hand, so this one runs on a real server.
#[tokio::test]
async fn postgres_unique_detail_exposes_every_column() -> anyhow::Result<()> {
    let adapter = PostgresAdapter::direct(POSTGRES_URL).await?;
    adapter.execute("DROP TABLE IF EXISTS engine_constraint_details", &[]).await?;
    adapter
        .execute(
            "CREATE TABLE engine_constraint_details (id BIGINT PRIMARY KEY, tenant_id BIGINT NOT NULL, \"Email\" TEXT NOT NULL, UNIQUE (tenant_id, \"Email\"))",
            &[],
        )
        .await?;
    adapter.execute("INSERT INTO engine_constraint_details VALUES (1, 1, 'a@dinoco.rs')", &[]).await?;

    let error = adapter
        .execute("INSERT INTO engine_constraint_details VALUES (2, 1, 'a@dinoco.rs')", &[])
        .await
        .expect_err("duplicate tenant and email");
    let error = DatabaseError::new(error);
    assert_eq!(error.constraint(), Some(DatabaseConstraintError::UniqueViolation));
    assert_eq!(error.constraint_details().table.as_deref(), Some("engine_constraint_details"));
    assert_eq!(error.constraint_details().columns, ["tenant_id", "Email"]);

    adapter.execute("DROP TABLE engine_constraint_details", &[]).await?;
    Ok(())
}
