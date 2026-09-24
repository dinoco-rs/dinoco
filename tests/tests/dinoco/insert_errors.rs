use dinoco::{CreateError, DatabaseConstraintError, Entity, find_many, insert_into, insert_many};
use dinoco_engine::{Backend, DinocoAdapter, DinocoClient, SqliteAdapter};

#[derive(Debug, Entity)]
#[dinoco(table_name = "insert_error_account")]
pub struct Account {
    id: String,
    email: String,
    balance: i64,
    nickname: Option<String>,
}

#[derive(Debug, Entity)]
#[dinoco(table_name = "insert_error_session")]
pub struct Session {
    id: String,
    account_id: String,
}

fn account(id: &str, email: &str) -> Account {
    let mut account = Account::new(id.to_string(), email.to_string(), 10);
    account.nickname = Some(id.to_string());
    account
}

#[tokio::test]
async fn insert_into_maps_unique_violations_with_table_and_columns() -> anyhow::Result<()> {
    let (client, path) = client("insert-into-unique").await?;
    insert_into::<Account>().value(&account("account-1", "taken@dinoco.rs")).execute(&client).await?;

    let error = insert_into::<Account>()
        .value(&account("account-2", "taken@dinoco.rs"))
        .execute(&client)
        .await
        .expect_err("duplicate email");
    let CreateError::UniqueViolation { ref table, ref columns, .. } = error else {
        panic!("expected a unique violation, got {error:?}");
    };
    assert_eq!(table.as_deref(), Some("insert_error_account"));
    assert_eq!(columns, &["email"]);
    assert!(error.is_unique_violation());
    assert_eq!(error.constraint(), Some(DatabaseConstraintError::UniqueViolation));
    assert_eq!(error.columns(), ["email"]);
    assert!(error.database_error().is_some());
    assert!(error.to_string().starts_with("unique constraint violated on `insert_error_account` (email)"));

    // A duplicated primary key is a unique violation too, also on the
    // returning path.
    let error = insert_into::<Account>()
        .value(&account("account-1", "other@dinoco.rs"))
        .returning::<Account>()
        .execute(&client)
        .await
        .expect_err("duplicate id");
    assert!(matches!(error, CreateError::UniqueViolation { ref columns, .. } if columns == &["id"]));

    // `?` still converts into `anyhow::Error`, and the typed error survives.
    let result: anyhow::Result<()> = async {
        insert_into::<Account>().value(&account("account-3", "taken@dinoco.rs")).execute(&client).await?;
        Ok(())
    }
    .await;
    let error = result.expect_err("duplicate email through anyhow");
    assert!(error.downcast_ref::<CreateError>().is_some_and(CreateError::is_unique_violation));

    drop(client);
    let _ = std::fs::remove_file(path);
    Ok(())
}

#[tokio::test]
async fn insert_many_maps_unique_violations_and_inserts_nothing() -> anyhow::Result<()> {
    let (client, path) = client("insert-many-unique").await?;

    let error = insert_many::<Account>()
        .values([&account("account-1", "same@dinoco.rs"), &account("account-2", "same@dinoco.rs")])
        .execute(&client)
        .await
        .expect_err("duplicate email in one batch");
    assert!(matches!(error, CreateError::UniqueViolation { ref columns, .. } if columns == &["email"]));
    assert!(find_many::<Account>().execute(&client).await?.is_empty());

    insert_into::<Account>().value(&account("account-1", "first@dinoco.rs")).execute(&client).await?;
    let error = insert_many::<Account>()
        .values([&account("account-2", "second@dinoco.rs"), &account("account-3", "first@dinoco.rs")])
        .pluck(|account| account.id)
        .execute(&client)
        .await
        .expect_err("duplicate email against an existing row");
    assert!(error.is_unique_violation());

    drop(client);
    let _ = std::fs::remove_file(path);
    Ok(())
}

#[tokio::test]
async fn insert_maps_foreign_key_not_null_and_check_violations() -> anyhow::Result<()> {
    let (client, path) = client("insert-constraints").await?;

    let error = insert_into::<Session>()
        .value(&Session::new("session-1".to_string(), "missing-account".to_string()))
        .execute(&client)
        .await
        .expect_err("missing referenced account");
    assert!(matches!(error, CreateError::ForeignKeyViolation { .. }), "got {error:?}");
    assert_eq!(error.constraint(), Some(DatabaseConstraintError::ForeignKeyViolation));

    let mut without_nickname = account("account-1", "null@dinoco.rs");
    without_nickname.nickname = None;
    let error =
        insert_many::<Account>().values([&without_nickname]).execute(&client).await.expect_err("nickname is NOT NULL");
    let CreateError::NotNullViolation { table, columns, .. } = error else {
        panic!("expected a not-null violation, got {error:?}");
    };
    assert_eq!(table.as_deref(), Some("insert_error_account"));
    assert_eq!(columns, ["nickname"]);

    let mut negative = account("account-2", "negative@dinoco.rs");
    negative.balance = -1;
    let error = insert_into::<Account>().value(&negative).execute(&client).await.expect_err("balance must be >= 0");
    let CreateError::CheckViolation { constraint, .. } = error else {
        panic!("expected a check violation, got {error:?}");
    };
    assert_eq!(constraint.as_deref(), Some("non_negative_balance"));

    drop(client);
    let _ = std::fs::remove_file(path);
    Ok(())
}

async fn client(name: &str) -> anyhow::Result<(DinocoClient, String)> {
    let path = format!("/private/tmp/dinoco-insert-errors-{name}-{}-{}.sqlite", std::process::id(), monotonic());
    let adapter = SqliteAdapter::new(path.clone()).await.map_err(anyhow::Error::msg)?;
    adapter
        .execute(
            "CREATE TABLE insert_error_account (
                id TEXT PRIMARY KEY NOT NULL,
                email TEXT NOT NULL UNIQUE,
                balance INTEGER NOT NULL,
                nickname TEXT NOT NULL,
                CONSTRAINT non_negative_balance CHECK (balance >= 0)
            )",
            &[],
        )
        .await?;
    adapter
        .execute(
            "CREATE TABLE insert_error_session (
                id TEXT PRIMARY KEY NOT NULL,
                account_id TEXT NOT NULL REFERENCES insert_error_account (id)
            )",
            &[],
        )
        .await?;
    Ok((DinocoClient::new(Backend::Sqlite(adapter)), path))
}

fn monotonic() -> u128 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
}
