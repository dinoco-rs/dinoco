use dinoco::{
    AtomicUpdateError, CreateError, DatabaseConstraintError, Entity, TransactionContext, TransactionError, count, delete,
    delete_many, find_and_update, find_first, find_many, insert_into, insert_many, transaction, update, update_many,
};
use dinoco_engine::{Backend, DinocoAdapter, DinocoClient, MigrationColumnType, SqliteAdapter};
use dinoco_tests::{column, create_table, nullable, primary};

#[derive(Debug, Entity)]
#[dinoco(table_name = "transaction_account")]
pub struct Account {
    id: String,
    email: String,
}

#[derive(Debug, Entity)]
#[dinoco(table_name = "transaction_account_session")]
pub struct AccountSession {
    id: String,
    account_id: String,
}

#[derive(Debug, Entity)]
#[dinoco(table_name = "transaction_author")]
pub struct Author {
    id: String,
    name: String,

    #[dinoco(one_to_many, foreign_key = "author_id", references = "id")]
    posts: Vec<Post>,
}

#[derive(Debug, Entity)]
#[dinoco(table_name = "transaction_post")]
pub struct Post {
    id: String,
    title: String,
    author_id: Option<String>,

    #[dinoco(many_to_one, foreign_key = "author_id", references = "id")]
    author: Option<Author>,
}

#[tokio::test]
async fn transaction_closure_commits_and_rolls_back_with_typed_errors() -> anyhow::Result<()> {
    let (client, path) = client("closure-api").await?;
    let committed = Account::new("account-committed".to_string(), "committed@dinoco.rs".to_string());

    transaction(&client, |tx| async move {
        insert_into::<Account>().value(&committed).execute(tx).await?;
        update::<Account>()
            .where_(|account| account.id.eq("account-committed"))
            .update(|account| account.email.set("updated@dinoco.rs"))
            .execute(tx)
            .await?;
        Ok(())
    })
    .await?;
    assert_eq!(
        find_first::<Account>()
            .where_(|account| account.id.eq("account-committed"))
            .execute(&client)
            .await?
            .expect("committed account")
            .email,
        "updated@dinoco.rs"
    );

    let rolled_back = Account::new("account-rolled-back".to_string(), "rollback@dinoco.rs".to_string());
    let result = transaction(&client, |tx| async move {
        insert_into::<Account>().value(&rolled_back).execute(tx).await?;
        find_and_update::<Account>()
            .where_(|account| account.id.eq("missing"))
            .update(|account| account.email.set("never@dinoco.rs"))
            .execute(tx)
            .await?;
        Ok(())
    })
    .await;
    assert!(matches!(result, Err(TransactionError::AtomicUpdate(AtomicUpdateError::RowNotAffected))));
    assert!(
        find_first::<Account>()
            .where_(|account| account.id.eq("account-rolled-back"))
            .execute(&client)
            .await?
            .is_none()
    );

    let first = Account::new("account-first".to_string(), "duplicate@dinoco.rs".to_string());
    let duplicate = Account::new("account-duplicate".to_string(), "duplicate@dinoco.rs".to_string());
    let result = transaction(&client, |tx| async move {
        insert_into::<Account>().value(&first).execute(tx).await?;
        insert_into::<Account>().value(&duplicate).execute(tx).await?;
        Ok(())
    })
    .await;
    let Err(TransactionError::Create(CreateError::UniqueViolation { table, columns, .. })) = result else {
        panic!("expected a unique violation, got {result:?}");
    };
    assert_eq!(table.as_deref(), Some("transaction_account"));
    assert_eq!(columns, vec!["email".to_string()]);
    assert!(find_first::<Account>().where_(|account| account.id.eq("account-first")).execute(&client).await?.is_none());

    drop(client);
    let _ = std::fs::remove_file(path);
    Ok(())
}

#[tokio::test]
async fn transaction_closure_classifies_update_and_delete_errors_and_rolls_back() -> anyhow::Result<()> {
    let (client, path) = client("typed-mutations").await?;
    let first = Account::new("account-update-first".to_string(), "first@dinoco.rs".to_string());
    let second = Account::new("account-update-second".to_string(), "second@dinoco.rs".to_string());
    insert_many::<Account>().values([&first, &second]).execute(&client).await?;

    let update_session = AccountSession::new("session-before-update".to_string(), first.id.clone());
    let result = transaction(&client, |tx| async move {
        insert_into::<AccountSession>().value(&update_session).execute(tx).await?;
        update::<Account>()
            .where_(|account| account.id.eq("account-update-second"))
            .update(|account| account.email.set("first@dinoco.rs"))
            .execute(tx)
            .await?;
        Ok(())
    })
    .await;
    assert!(matches!(
        result,
        Err(TransactionError::Update(dinoco::UpdateError::Constraint {
            kind: DatabaseConstraintError::UniqueViolation,
            ..
        }))
    ));
    assert!(
        find_first::<AccountSession>()
            .where_(|session| session.id.eq("session-before-update"))
            .execute(&client)
            .await?
            .is_none()
    );

    let Backend::Sqlite(adapter) = &client.backend else { unreachable!("sqlite transaction test") };
    adapter
        .execute(
            "CREATE TRIGGER reject_transaction_account_delete BEFORE DELETE ON transaction_account BEGIN SELECT RAISE(ABORT, 'delete rejected'); END",
            &[],
        )
        .await?;

    let delete_session = AccountSession::new("session-before-delete".to_string(), first.id.clone());
    let result = transaction(&client, |tx| async move {
        insert_into::<AccountSession>().value(&delete_session).execute(tx).await?;
        delete::<Account>().where_(|account| account.id.eq("account-update-first")).execute(tx).await?;
        Ok(())
    })
    .await;
    assert!(matches!(result, Err(TransactionError::Delete(dinoco::DeleteError::Database(_)))));
    assert!(
        find_first::<AccountSession>()
            .where_(|session| session.id.eq("session-before-delete"))
            .execute(&client)
            .await?
            .is_none()
    );
    assert!(
        find_first::<Account>()
            .where_(|account| account.id.eq("account-update-first"))
            .execute(&client)
            .await?
            .is_some()
    );

    drop(client);
    let _ = std::fs::remove_file(path);
    Ok(())
}

#[tokio::test]
async fn transaction_reads_see_uncommitted_writes_and_load_includes() -> anyhow::Result<()> {
    let (client, path) = client("reads").await?;
    create_blog_tables(&client).await?;

    let author = Author::new("author-1".to_string(), "Ada".to_string());
    let mut first = Post::new("post-1".to_string(), "Engines".to_string());
    first.author_id = Some(author.id.clone());
    let mut second = Post::new("post-2".to_string(), "Analytical".to_string());
    second.author_id = Some(author.id.clone());

    let titles = transaction(&client, |tx| async move {
        insert_into::<Author>().value(&author).execute(tx).await?;
        insert_many::<Post>().values([&first, &second]).execute(tx).await?;

        let found = find_first::<Author>()
            .where_(|author| author.id.eq("author-1"))
            .includes(|author| author.posts().order_by(|post| post.title.asc()))
            .execute(tx)
            .await?
            .expect("author inserted in this transaction");
        assert_eq!(found.posts.iter().map(|post| post.title.as_str()).collect::<Vec<_>>(), ["Analytical", "Engines"]);

        let posts = find_many::<Post>().includes(|post| post.author()).execute(tx).await?;
        assert_eq!(posts.len(), 2);
        assert!(posts.iter().all(|post| post.author.as_ref().is_some_and(|author| author.name == "Ada")));

        let name = find_first::<Author>().pluck(|author| author.name).execute(tx).await?;
        assert_eq!(name.as_deref(), Some("Ada"));

        let lengths = find_many::<Post>().transform(|post| post.title.len()).execute(tx).await?;
        assert_eq!(lengths.iter().sum::<usize>(), "Engines".len() + "Analytical".len());

        find_many::<Post>().order_by(|post| post.title.asc()).pluck(|post| post.title).execute(tx).await
    })
    .await?;
    assert_eq!(titles, ["Analytical", "Engines"]);
    assert_eq!(find_many::<Post>().execute(&client).await?.len(), 2);

    let author = Author::new("author-2".to_string(), "Grace".to_string());
    let result: Result<(), _> = transaction(&client, |tx| async move {
        insert_into::<Author>().value(&author).execute(tx).await?;
        let visible = find_first::<Author>().where_(|author| author.id.eq("author-2")).execute(tx).await?;
        assert!(visible.is_some(), "a read inside the transaction sees its own insert");

        anyhow::bail!("abort after reading")
    })
    .await;
    assert!(matches!(result, Err(TransactionError::Operation(_))));
    assert!(find_first::<Author>().where_(|author| author.id.eq("author-2")).execute(&client).await?.is_none());

    drop(client);
    let _ = std::fs::remove_file(path);
    Ok(())
}

#[tokio::test]
async fn transaction_reads_reject_a_context_used_after_its_closure() -> anyhow::Result<()> {
    let (client, path) = client("leaked-reads").await?;

    let leaked = transaction(&client, |tx| async move { Ok(tx) }).await?;
    let error = find_many::<Account>().execute(leaked).await.expect_err("context outside its closure");
    assert!(error.to_string().contains("outside its transaction closure"));

    drop(client);
    let _ = std::fs::remove_file(path);
    Ok(())
}

async fn create_blog_tables(client: &DinocoClient) -> anyhow::Result<()> {
    let Backend::Sqlite(adapter) = &client.backend else { unreachable!("sqlite transaction test") };
    create_table(
        adapter,
        "transaction_author",
        vec![primary(column("id", MigrationColumnType::String)), column("name", MigrationColumnType::String)],
    )
    .await?;
    create_table(
        adapter,
        "transaction_post",
        vec![
            primary(column("id", MigrationColumnType::String)),
            column("title", MigrationColumnType::String),
            nullable(column("author_id", MigrationColumnType::String)),
        ],
    )
    .await
}

async fn client(name: &str) -> anyhow::Result<(DinocoClient, String)> {
    let path = format!("/private/tmp/dinoco-transaction-{name}-{}-{}.sqlite", std::process::id(), monotonic());
    let adapter = SqliteAdapter::new(path.clone()).await.map_err(anyhow::Error::msg)?;
    let mut email = column("email", MigrationColumnType::String);
    email.unique = true;
    create_table(&adapter, "transaction_account", vec![primary(column("id", MigrationColumnType::String)), email])
        .await?;
    create_table(
        &adapter,
        "transaction_account_session",
        vec![primary(column("id", MigrationColumnType::String)), column("account_id", MigrationColumnType::String)],
    )
    .await?;
    Ok((DinocoClient::new(Backend::Sqlite(adapter)), path))
}

fn monotonic() -> u128 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
}

#[allow(dead_code)]
fn public_operation_futures_are_send(client: &DinocoClient) {
    fn assert_send<T: Send>(_: T) {}

    let account = Account::new("account-1".to_string(), "send@dinoco.rs".to_string());

    assert_send(find_first::<Account>().execute(client));
    assert_send(find_many::<Account>().execute(client));
    assert_send(find_first::<Account>().execute(TransactionContext));
    assert_send(find_many::<Author>().includes(|author| author.posts()).execute(TransactionContext));
    assert_send(count::<Account>().execute(client));
    assert_send(insert_into::<Account>().values(&account).execute(client));
    assert_send(insert_into::<Account>().values(&account).returning::<Account>().execute(client));
    assert_send(insert_many::<Account>().values([&account]).execute(client));
    assert_send(insert_many::<Account>().values([&account]).returning::<Account>().execute(client));
    assert_send(
        update::<Account>()
            .where_(|item| item.id.eq("account-1"))
            .update(|item| item.email.set("updated@dinoco.rs"))
            .execute(client),
    );
    assert_send(
        update::<Account>()
            .where_(|item| item.id.eq("account-1"))
            .update(|item| item.email.set("updated@dinoco.rs"))
            .returning::<Account>()
            .execute(client),
    );
    assert_send(update_many::<Account>().update(|item| item.email.set("updated@dinoco.rs")).execute(client));
    assert_send(
        update_many::<Account>()
            .update(|item| item.email.set("updated@dinoco.rs"))
            .returning::<Account>()
            .execute(client),
    );
    assert_send(
        find_and_update::<Account>()
            .where_(|item| item.id.eq("account-1"))
            .update(|item| item.email.set("updated@dinoco.rs"))
            .execute(client),
    );
    assert_send(delete::<Account>().where_(|item| item.id.eq("account-1")).execute(client));
    assert_send(delete::<Account>().where_(|item| item.id.eq("account-1")).returning::<Account>().execute(client));
    assert_send(delete_many::<Account>().execute(client));
    assert_send(delete_many::<Account>().returning::<Account>().execute(client));
}
