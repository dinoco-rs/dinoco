use std::sync::{Arc, Mutex};

use dinoco::{
    AtomicUpdateError, Backend, CreateError, DatabaseConstraintError, DinocoAdapter, DinocoClient, DinocoEntity,
    Entity, QueryMode, TestAmbient, count, delete, delete_many, find_and_update, find_first, find_many, insert_into,
    insert_many, remove_test_methods, setup_test_methods, transaction, update, update_many,
};

#[derive(Debug, Clone, Entity)]
#[dinoco(table_name = "account")]
pub struct Account {
    #[dinoco(primary_key)]
    id: String,
    email: String,
    name: String,

    #[dinoco(one_to_many, foreign_key = "account_id", references = "id")]
    sessions: Vec<Session>,
}

#[derive(Debug, Clone, Entity)]
#[dinoco(table_name = "session")]
pub struct Session {
    #[dinoco(primary_key)]
    id: String,
    account_id: Option<String>,
    #[dinoco(default = true)]
    active: bool,

    #[dinoco(many_to_one, foreign_key = "account_id", references = "id")]
    account: Option<Account>,
}

const SCHEMA: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/test_ambient/schema.dinoco");

async fn ambient() -> anyhow::Result<DinocoClient> {
    TestAmbient::new().schema(SCHEMA).create().await
}

fn account(id: &str, email: &str) -> Account {
    Account::new(id.to_string(), email.to_string(), id.to_string())
}

fn session(id: &str, account_id: &str) -> Session {
    let mut session = Session::new(id.to_string());
    session.account_id = Some(account_id.to_string());
    session
}

/// Collects one line per callback call, so a test can assert on the exact
/// sequence of operations.
#[derive(Clone, Default)]
struct Events(Arc<Mutex<Vec<String>>>);

impl Events {
    fn push(&self, event: String) {
        self.0.lock().unwrap().push(event);
    }

    fn take(&self) -> Vec<String> {
        std::mem::take(&mut *self.0.lock().unwrap())
    }
}

fn outcome<T: std::fmt::Debug>(value: Option<T>, error: Option<&dinoco::DatabaseError>) -> String {
    match (value, error) {
        (Some(value), None) => format!("{value:?}"),
        (None, Some(error)) => format!("error={:?}", error.constraint()),
        (value, _) => panic!("a callback got both or neither result and error: {value:?}"),
    }
}

fn tx(query: &dinoco::ExecutedQuery) -> &'static str {
    if query.in_transaction { " tx" } else { "" }
}

/// Installs every callback for `M`, each one writing a short description of
/// what it received into `events`.
fn record<M: DinocoEntity>(client: &DinocoClient, events: &Events) {
    let (insert, update, delete, find) = (events.clone(), events.clone(), events.clone(), events.clone());
    setup_test_methods::<M>(client)
        .on_insert(move |rows, query, error| {
            insert.push(format!("{} insert {}{}", query.table, outcome(rows.map(<[_]>::len), error), tx(query)))
        })
        .on_update(move |affected, query, error| {
            update.push(format!("{} update {}{}", query.table, outcome(affected, error), tx(query)))
        })
        .on_delete(move |affected, query, error| {
            delete.push(format!("{} delete {}{}", query.table, outcome(affected, error), tx(query)))
        })
        .on_find(move |rows, query, error| {
            find.push(format!("{} find {}{}", query.table, outcome(rows, error), tx(query)))
        });
}

// ---------------------------------------------------------------------------
// create_test_ambient / TestAmbient
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_ambient_builds_the_schema_in_an_isolated_memory_database() -> anyhow::Result<()> {
    // The fixture schema targets PostgreSQL; the ambient still builds it on SQLite.
    let client = ambient().await?;
    let other = ambient().await?;

    insert_into::<Account>().values(&account("account-1", "ada@dinoco.rs")).execute(&client).await?;
    insert_many::<Session>()
        .values([&session("session-1", "account-1"), &session("session-2", "account-1")])
        .execute(&client)
        .await?;

    let found =
        find_first::<Account>().includes(|account| account.sessions()).execute(&client).await?.expect("account");
    assert_eq!(found.sessions.len(), 2);
    assert!(found.sessions.iter().all(|session| session.active), "schema defaults are applied");
    assert!(find_many::<Account>().execute(&other).await?.is_empty(), "each ambient has its own database");

    // Constraints come from the schema: `@unique` and the relation's foreign key.
    let error =
        insert_into::<Account>().values(&account("account-2", "ada@dinoco.rs")).execute(&client).await.unwrap_err();
    assert!(matches!(error, CreateError::UniqueViolation { ref columns, .. } if columns == &["email"]), "{error:?}");
    let error = insert_into::<Session>().values(&session("session-3", "missing")).execute(&client).await.unwrap_err();
    assert_eq!(error.constraint(), Some(DatabaseConstraintError::ForeignKeyViolation));

    Ok(())
}

#[tokio::test]
async fn test_ambient_creates_enums_defaults_and_composite_indexes() -> anyhow::Result<()> {
    let client = ambient().await?;
    let Backend::Sqlite(adapter) = &client.backend else { unreachable!("the test ambient is SQLite") };

    let tables = adapter
        .query_count(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name IN ('account', 'session', 'audit_log')",
            &[],
        )
        .await?;
    assert_eq!(tables, 3);

    // Every default of `audit_log` is filled by the database itself.
    adapter.execute("INSERT INTO audit_log (tenant_id, slug) VALUES ('tenant-1', 'first')", &[]).await?;
    let defaults = adapter
        .query_count(
            "SELECT COUNT(*) FROM audit_log WHERE id = 1 AND kind = 'CREATED' AND created_at IS NOT NULL AND payload IS NULL",
            &[],
        )
        .await?;
    assert_eq!(defaults, 1);

    // `@@uniques([tenant_id, slug])` is enforced; the same slug in another tenant is fine.
    adapter.execute("INSERT INTO audit_log (tenant_id, slug) VALUES ('tenant-2', 'first')", &[]).await?;
    let duplicate = adapter.execute("INSERT INTO audit_log (tenant_id, slug) VALUES ('tenant-1', 'first')", &[]).await;
    assert_eq!(
        dinoco::DatabaseError::new(duplicate.unwrap_err()).constraint(),
        Some(DatabaseConstraintError::UniqueViolation)
    );

    // `@@indexes([created_at])` exists as a plain index.
    let indexes = adapter
        .query_count(
            "SELECT COUNT(*) FROM pragma_index_list('audit_log') AS list, pragma_index_info(list.name) AS info WHERE list.\"unique\" = 0 AND info.name = 'created_at'",
            &[],
        )
        .await?;
    assert_eq!(indexes, 1);

    Ok(())
}

#[tokio::test]
async fn test_ambient_shares_one_database_between_pooled_connections_and_transactions() -> anyhow::Result<()> {
    let client = ambient().await?;

    transaction(&client, |tx| async move {
        insert_into::<Account>().values(&account("account-1", "ada@dinoco.rs")).execute(tx).await?;
        Ok(())
    })
    .await?;
    let rolled_back: Result<(), _> = transaction(&client, |tx| async move {
        insert_into::<Account>().values(&account("account-2", "grace@dinoco.rs")).execute(tx).await?;
        anyhow::bail!("roll back")
    })
    .await;
    assert!(rolled_back.is_err());

    // Several reads at once use several pooled connections on the same database.
    let (first, second, all) = tokio::try_join!(
        find_first::<Account>().where_(|account| account.id.eq("account-1")).execute(&client),
        find_first::<Account>().where_(|account| account.id.eq("account-2")).execute(&client),
        find_many::<Account>().execute(&client),
    )?;
    assert!(first.is_some());
    assert!(second.is_none());
    assert_eq!(all.len(), 1);

    Ok(())
}

#[tokio::test]
async fn test_ambients_run_in_parallel_without_sharing_rows() -> anyhow::Result<()> {
    let tasks = (0..8).map(|index| {
        tokio::spawn(async move {
            let client = ambient().await?;
            let accounts = (0..=index)
                .map(|row| account(&format!("account-{row}"), &format!("{row}@dinoco.rs")))
                .collect::<Vec<_>>();
            insert_many::<Account>().values(&accounts).execute(&client).await?;
            anyhow::Ok((index, count::<Account>().execute(&client).await?.total))
        })
    });

    for task in tasks.collect::<Vec<_>>() {
        let (index, total) = task.await??;
        assert_eq!(total, index as i64 + 1, "ambient {index} only sees its own rows");
    }

    Ok(())
}

#[tokio::test]
async fn test_ambient_applies_the_workspace_and_its_config() -> anyhow::Result<()> {
    let dir = std::env::temp_dir().join(format!("dinoco-test-ambient-workspace-{}", std::process::id()));
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("schema.dinoco");
    let schema = std::fs::read_to_string(SCHEMA)?.replace(
        "config {\n    database = \"postgresql\"\n    database_url = env(\"DATABASE_URL\")\n}",
        r#"config {
    workspace {
        dev {
            database     = "sqlite"
            database_url = env("DEV_DATABASE_URL")
            query_mode   = "single_query"
        }

        prod {
            database     = "postgresql"
            database_url = env("PROD_DATABASE_URL")
        }
    }
}"#,
    );
    assert!(schema.contains("workspace {"), "fixture config block was replaced");
    std::fs::write(&path, schema)?;

    let dev = TestAmbient::new().schema(&path).workspace("dev").create().await?;
    assert_eq!(dev.query_mode(), QueryMode::SingleQuery);
    insert_into::<Account>().values(&account("account-1", "ada@dinoco.rs")).execute(&dev).await?;
    let (accounts, first) = dinoco::find_batch((
        find_many::<Account>(),
        find_first::<Account>().where_(|account| account.id.eq("account-1")),
    ))
    .execute(&dev)
    .await?;
    assert_eq!(accounts.len(), 1);
    assert!(first.is_some(), "single_query find_batch works on the ambient");

    let prod = TestAmbient::new().schema(&path).workspace("prod").create().await?;
    assert_eq!(prod.query_mode(), QueryMode::BatchQuery);

    let Err(error) = TestAmbient::new().schema(&path).workspace("staging").create().await else {
        panic!("an unknown workspace must fail");
    };
    assert!(error.to_string().contains("workspace `staging` was not found"), "{error:#}");

    let _ = std::fs::remove_dir_all(dir);
    Ok(())
}

#[tokio::test]
async fn test_ambient_reports_a_missing_schema() {
    let Err(error) = TestAmbient::new().schema("does/not/exist.dinoco").create().await else {
        panic!("a missing schema must fail");
    };
    assert!(format!("{error:#}").contains("does/not/exist.dinoco"), "{error:#}");
}

#[tokio::test]
async fn test_ambient_reports_an_invalid_schema() -> anyhow::Result<()> {
    let dir = std::env::temp_dir().join(format!("dinoco-test-ambient-invalid-{}", std::process::id()));
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("schema.dinoco");
    std::fs::write(&path, "model Broken {\n    id String @id\n    owner Missing?\n}\n")?;

    let Err(error) = TestAmbient::new().schema(&path).create().await else {
        panic!("an invalid schema must fail");
    };
    assert!(format!("{error:#}").contains("failed to compile"), "{error:#}");

    let _ = std::fs::remove_dir_all(dir);
    Ok(())
}

// ---------------------------------------------------------------------------
// setup_test_methods::<M> / remove_test_methods::<M>
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_methods_only_observe_their_model() -> anyhow::Result<()> {
    let client = ambient().await?;
    let events = Events::default();
    record::<Account>(&client, &events);

    insert_into::<Account>().values(&account("account-1", "ada@dinoco.rs")).execute(&client).await?;
    insert_into::<Session>().values(&session("session-1", "account-1")).execute(&client).await?;
    find_many::<Session>().execute(&client).await?;
    delete_many::<Session>().execute(&client).await?;
    find_many::<Account>().execute(&client).await?;

    assert_eq!(events.take(), ["account insert 1", "account find 1"]);

    Ok(())
}

#[tokio::test]
async fn test_methods_report_rows_sql_and_params() -> anyhow::Result<()> {
    let client = ambient().await?;
    let seen = Arc::new(Mutex::new(Vec::new()));

    let inserts = seen.clone();
    let finds = seen.clone();
    setup_test_methods::<Account>(&client)
        .on_insert(move |rows, query, _| {
            inserts.lock().unwrap().push((query.sql.clone(), query.params.len(), rows.map(<[_]>::to_vec)));
        })
        .on_find(move |_, query, _| finds.lock().unwrap().push((query.sql.clone(), query.params.len(), None)));

    insert_many::<Account>()
        .values([&account("account-1", "ada@dinoco.rs"), &account("account-2", "grace@dinoco.rs")])
        .execute(&client)
        .await?;
    find_first::<Account>().where_(|account| account.email.eq("ada@dinoco.rs")).execute(&client).await?;

    let seen = seen.lock().unwrap();
    let (sql, params, rows) = &seen[0];
    assert!(sql.starts_with("INSERT INTO account"), "{sql}");
    assert_eq!(*params, 6, "two rows of three columns");
    let rows = rows.as_ref().expect("inserted rows");
    assert_eq!(rows.len(), 2, "one callback call for the whole batch");
    assert_eq!(rows[0]["email"], "ada@dinoco.rs");
    assert_eq!(rows[1]["id"], "account-2");

    let (sql, params, _) = &seen[1];
    assert!(sql.starts_with("SELECT") && sql.contains("FROM account"), "{sql}");
    assert!(*params >= 1, "the email filter is bound as a parameter");

    Ok(())
}

#[tokio::test]
async fn test_methods_observe_nested_inserts_and_includes() -> anyhow::Result<()> {
    let client = ambient().await?;
    let events = Events::default();
    record::<Account>(&client, &events);
    record::<Session>(&client, &events);

    let mut ada = account("account-1", "ada@dinoco.rs");
    ada.sessions = vec![session("session-1", "account-1"), session("session-2", "account-1")];
    insert_into::<Account>().values(&ada).execute(&client).await?;
    let nested = events.take();
    assert!(nested.contains(&"account insert 1".to_string()), "{nested:?}");
    assert!(nested.contains(&"session insert 2".to_string()), "{nested:?}");

    find_first::<Account>().includes(|account| account.sessions()).execute(&client).await?;
    find_many::<Session>().includes(|session| session.account()).execute(&client).await?;

    // A relation query returns one row per parent that points at the related
    // row, so both sessions report their (shared) account.
    assert_eq!(events.take(), ["account find 1", "session find 2", "session find 2", "account find 2"]);

    Ok(())
}

#[tokio::test]
async fn test_methods_report_failures_with_the_database_error() -> anyhow::Result<()> {
    let client = ambient().await?;
    let events = Events::default();
    record::<Session>(&client, &events);
    let details = Arc::new(Mutex::new(None));

    let captured = details.clone();
    let account_events = events.clone();
    setup_test_methods::<Account>(&client).on_insert(move |rows, query, error| {
        account_events.push(format!("{} insert {:?} failed={}", query.table, rows.map(<[_]>::len), error.is_some()));
        if let Some(error) = error {
            *captured.lock().unwrap() = Some(error.constraint_details().clone());
        }
    });

    insert_into::<Account>().values(&account("account-1", "ada@dinoco.rs")).execute(&client).await?;
    let result = insert_into::<Account>().values(&account("account-2", "ada@dinoco.rs")).execute(&client).await;
    assert!(result.unwrap_err().is_unique_violation(), "the builder still returns its own typed error");
    let _ = insert_into::<Session>().values(&session("session-1", "missing")).execute(&client).await;

    assert_eq!(
        events.take(),
        [
            "account insert Some(1) failed=false",
            "account insert None failed=true",
            "session insert error=Some(ForeignKeyViolation)",
        ]
    );
    let details = details.lock().unwrap().clone().expect("unique violation details");
    assert_eq!(details.table.as_deref(), Some("account"));
    assert_eq!(details.columns, ["email"]);

    Ok(())
}

#[tokio::test]
async fn test_methods_report_affected_rows_for_every_write_builder() -> anyhow::Result<()> {
    let client = ambient().await?;
    let events = Events::default();
    insert_many::<Account>()
        .values([
            &account("account-1", "ada@dinoco.rs"),
            &account("account-2", "grace@dinoco.rs"),
            &account("account-3", "alan@dinoco.rs"),
        ])
        .execute(&client)
        .await?;
    record::<Account>(&client, &events);

    update::<Account>()
        .where_(|account| account.id.eq("account-1"))
        .update(|account| account.name.set("Ada"))
        .execute(&client)
        .await?;
    update_many::<Account>().update(|account| account.name.set("Everyone")).execute(&client).await?;
    update_many::<Account>()
        .where_(|account| account.id.eq("missing"))
        .update(|account| account.name.set("Nobody"))
        .returning::<Account>()
        .execute(&client)
        .await?;
    find_and_update::<Account>()
        .where_(|account| account.id.eq("account-2"))
        .update(|account| account.name.set("Grace"))
        .execute(&client)
        .await?;
    let missing = find_and_update::<Account>()
        .where_(|account| account.id.eq("missing"))
        .update(|account| account.name.set("Nobody"))
        .execute(&client)
        .await;
    assert!(matches!(missing, Err(AtomicUpdateError::RowNotAffected)));
    delete::<Account>().where_(|account| account.id.eq("account-3")).returning::<Account>().execute(&client).await?;
    delete_many::<Account>().execute(&client).await?;

    assert_eq!(
        events.take(),
        [
            "account update 1",
            "account update 3",
            "account update 0",
            "account update 1",
            "account update 0",
            "account delete 1",
            "account delete 2",
        ]
    );

    Ok(())
}

#[tokio::test]
async fn test_methods_observe_transactions_including_rolled_back_work() -> anyhow::Result<()> {
    let client = ambient().await?;
    let events = Events::default();
    insert_into::<Account>().values(&account("account-1", "ada@dinoco.rs")).execute(&client).await?;
    record::<Account>(&client, &events);
    record::<Session>(&client, &events);

    transaction(&client, |tx| async move {
        insert_many::<Session>()
            .values([&session("session-1", "account-1"), &session("session-2", "account-1")])
            .execute(tx)
            .await?;
        update::<Session>()
            .where_(|session| session.id.eq("session-1"))
            .update(|session| session.active.set(false))
            .execute(tx)
            .await?;
        find_first::<Account>().includes(|account| account.sessions()).execute(tx).await?;
        Ok(())
    })
    .await?;
    let _: Result<(), _> = transaction(&client, |tx| async move {
        delete::<Session>().where_(|session| session.id.eq("session-2")).execute(tx).await?;
        anyhow::bail!("roll back the delete")
    })
    .await;

    assert_eq!(
        events.take(),
        ["session insert 2 tx", "session update 1 tx", "account find 1 tx", "session find 2 tx", "session delete 1 tx",]
    );
    assert_eq!(count::<Session>().execute(&client).await?.total, 2, "the observed delete was rolled back");

    Ok(())
}

#[tokio::test]
async fn test_methods_can_be_extended_replaced_and_removed_per_model() -> anyhow::Result<()> {
    let client = ambient().await?;
    let events = Events::default();

    let first = events.clone();
    setup_test_methods::<Account>(&client).on_insert(move |_, _, _| first.push("first insert".to_string()));
    let finds = events.clone();
    setup_test_methods::<Account>(&client).on_find(move |_, _, _| finds.push("find".to_string()));

    insert_into::<Account>().values(&account("account-1", "ada@dinoco.rs")).execute(&client).await?;
    find_many::<Account>().execute(&client).await?;
    assert_eq!(events.take(), ["first insert", "find"], "a second setup keeps the callbacks already installed");

    let second = events.clone();
    setup_test_methods::<Account>(&client).on_insert(move |_, _, _| second.push("second insert".to_string()));
    insert_into::<Account>().values(&account("account-2", "grace@dinoco.rs")).execute(&client).await?;
    assert_eq!(events.take(), ["second insert"], "setting a callback again replaces it");

    record::<Session>(&client, &events);
    remove_test_methods::<Account>(&client);
    insert_into::<Account>().values(&account("account-3", "alan@dinoco.rs")).execute(&client).await?;
    find_many::<Account>().execute(&client).await?;
    insert_into::<Session>().values(&session("session-1", "account-1")).execute(&client).await?;
    assert_eq!(events.take(), ["session insert 1"], "removing Account keeps Session's callbacks");

    remove_test_methods::<Session>(&client);
    remove_test_methods::<Session>(&client);
    insert_into::<Session>().values(&session("session-2", "account-1")).execute(&client).await?;
    assert!(events.take().is_empty());
    assert!(client.query_hooks().is_none(), "no callback left on the client");

    Ok(())
}

#[tokio::test]
async fn test_methods_belong_to_one_client() -> anyhow::Result<()> {
    let observed = ambient().await?;
    let other = ambient().await?;
    let events = Events::default();
    record::<Account>(&observed, &events);

    insert_into::<Account>().values(&account("account-1", "ada@dinoco.rs")).execute(&other).await?;
    find_many::<Account>().execute(&other).await?;
    assert!(events.take().is_empty());

    Ok(())
}

#[tokio::test]
async fn test_methods_skip_count_and_exists() -> anyhow::Result<()> {
    let client = ambient().await?;
    let events = Events::default();
    record::<Account>(&client, &events);

    count::<Account>().execute(&client).await?;
    dinoco::exists::<Account>().execute(&client).await?;
    assert!(events.take().is_empty(), "documented limit: count and exists are not reported");

    Ok(())
}
