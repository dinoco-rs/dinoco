//! One-to-one relations seen from both sides: the owning side keeps the
//! unique foreign key (`#[dinoco(one_to_one)]`), the other side reads it from
//! the target (`#[dinoco(one_to_one, inverse)]`). Both sides are boxed, as the
//! generated models are, because each struct contains the other.

use dinoco::{Entity, find_first, find_many, insert_into};
use dinoco_engine::{Backend, DinocoAdapter, DinocoClient, MigrationColumnType, SqliteAdapter};
use dinoco_tests::{column, create_table, primary};

#[derive(Debug, Clone, Entity)]
#[dinoco(table_name = "oto_account")]
struct Account {
    #[dinoco(primary_key)]
    id: String,
    email: String,

    #[dinoco(one_to_one, inverse, foreign_key = "account_id", references = "id")]
    profile: Option<Box<Profile>>,
}

#[derive(Debug, Clone, Entity)]
#[dinoco(table_name = "oto_profile")]
struct Profile {
    #[dinoco(primary_key)]
    id: String,
    account_id: String,
    bio: String,

    #[dinoco(one_to_one, foreign_key = "account_id", references = "id")]
    account: Option<Box<Account>>,
}

#[tokio::test]
async fn includes_load_one_to_one_relations_from_both_sides() -> anyhow::Result<()> {
    let (client, path) = client("one-to-one-includes").await?;
    create_tables(&client).await?;

    for (account_id, email) in [("account-a", "a@example.com"), ("account-b", "b@example.com")] {
        let account = Account::new(account_id.to_string(), email.to_string());
        insert_into::<Account>().values(&account).execute(&client).await?;
    }
    let profile = Profile::new("profile-a".to_string(), "account-a".to_string(), "Hello".to_string());
    insert_into::<Profile>().values(&profile).execute(&client).await?;

    let accounts = find_many::<Account>()
        .includes(|account| account.profile())
        .order_by(|account| account.id.asc())
        .execute(&client)
        .await?;
    assert_eq!(accounts.len(), 2);
    assert_eq!(accounts[0].profile.as_ref().map(|profile| profile.id.as_str()), Some("profile-a"));
    assert!(accounts[1].profile.is_none(), "an account without a profile loads None");

    let profile = find_first::<Profile>()
        .where_(|profile| profile.id.eq("profile-a"))
        .includes(|profile| profile.account().includes(|account| account.profile()))
        .execute(&client)
        .await?
        .expect("profile");
    let account = profile.account.as_ref().expect("profile.account include");
    assert_eq!(account.email, "a@example.com");
    assert_eq!(account.profile.as_ref().map(|profile| profile.bio.as_str()), Some("Hello"));

    let _ = std::fs::remove_file(path);
    Ok(())
}

#[tokio::test]
async fn inserting_the_non_owning_side_inserts_and_binds_the_owner() -> anyhow::Result<()> {
    let (client, path) = client("one-to-one-nested-insert").await?;
    create_tables(&client).await?;

    let mut account = Account::new("account-a".to_string(), "a@example.com".to_string());
    account.profile = Some(Box::new(Profile::new("profile-a".to_string(), String::new(), "Nested".to_string())));
    insert_into::<Account>().values(&account).execute(&client).await?;

    let profile = find_first::<Profile>()
        .where_(|profile| profile.id.eq("profile-a"))
        .execute(&client)
        .await?
        .expect("nested profile");
    assert_eq!(profile.account_id, "account-a");
    assert_eq!(profile.bio, "Nested");

    let _ = std::fs::remove_file(path);
    Ok(())
}

async fn create_tables(client: &DinocoClient) -> anyhow::Result<()> {
    let Backend::Sqlite(adapter) = &client.backend else { unreachable!("sqlite test") };

    create_table(
        adapter,
        "oto_account",
        vec![primary(column("id", MigrationColumnType::String)), column("email", MigrationColumnType::String)],
    )
    .await?;
    create_table(
        adapter,
        "oto_profile",
        vec![
            primary(column("id", MigrationColumnType::String)),
            column("account_id", MigrationColumnType::String),
            column("bio", MigrationColumnType::String),
        ],
    )
    .await
}

async fn client(name: &str) -> anyhow::Result<(DinocoClient, String)> {
    let path = format!("/private/tmp/dinoco-{name}-{}-{}.sqlite", std::process::id(), monotonic());
    let adapter = SqliteAdapter::new(path.clone()).await.map_err(anyhow::Error::msg)?;
    Ok((DinocoClient::new(Backend::Sqlite(adapter)), path))
}

fn monotonic() -> u128 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
}
