use dinoco::{Entity, count, find_and_update, find_many, insert_into, insert_many};
use dinoco_engine::{Backend, DinocoAdapter, DinocoClient, MigrationColumnType, SqliteAdapter};
use dinoco_tests::{column, create_table, nullable, primary};

#[derive(Debug, Entity)]
#[dinoco(table_name = "relation_user")]
pub struct User {
    #[dinoco(auto_generate = uuid)]
    id: String,
    email: String,

    #[dinoco(one_to_many, foreign_key = "user_id", references = "id")]
    tokens: Vec<Token>,
}

#[derive(Debug, Entity)]
#[dinoco(table_name = "relation_token")]
pub struct Token {
    #[dinoco(auto_generate = uuid)]
    id: String,
    user_id: Option<String>,

    #[dinoco(one_to_many, foreign_key = "user_id", references = "id")]
    user: Option<User>,
}

#[derive(Debug, Entity)]
#[dinoco(table_name = "relation_business")]
pub struct Business {
    #[dinoco(primary_key)]
    id: String,
    name: String,

    #[dinoco(
        many_to_many,
        foreign_key = "id",
        references = "id",
        join_table = "_relation_business_to_relation_system",
        parent_field = "id",
        join_parent_field = "business_id",
        join_child_field = "system_id"
    )]
    systems: Vec<System>,

    #[dinoco(
        many_to_many_key,
        join_table = "_relation_business_to_relation_system",
        parent_field = "id",
        join_parent_field = "business_id",
        join_child_field = "system_id"
    )]
    system_id: Option<String>,
}

#[derive(Debug, Entity)]
#[dinoco(table_name = "relation_system")]
pub struct System {
    #[dinoco(primary_key)]
    id: String,
    name: String,

    #[dinoco(
        many_to_many,
        foreign_key = "id",
        references = "id",
        join_table = "_relation_business_to_relation_system",
        parent_field = "id",
        join_parent_field = "system_id",
        join_child_field = "business_id"
    )]
    businesses: Vec<Business>,

    #[dinoco(
        many_to_many_key,
        join_table = "_relation_business_to_relation_system",
        parent_field = "id",
        join_parent_field = "system_id",
        join_child_field = "business_id"
    )]
    business_id: Option<String>,
}

#[tokio::test]
async fn insert_resolves_vec_relations_without_with_relation() -> anyhow::Result<()> {
    let (client, path) = client("adapters-relations").await?;
    let Backend::Sqlite(adapter) = &client.backend else { unreachable!("sqlite test") };
    create_table(
        adapter,
        "relation_user",
        vec![primary(column("id", MigrationColumnType::String)), column("email", MigrationColumnType::String)],
    )
    .await?;
    create_table(
        adapter,
        "relation_token",
        vec![
            primary(column("id", MigrationColumnType::String)),
            nullable(column("user_id", MigrationColumnType::String)),
        ],
    )
    .await?;

    let mut user = User::new("relations@dinoco.rs".to_string());
    user.tokens = vec![Token::new(), Token::new(), Token::new()];
    insert_into::<User>().values(&user).execute(&client).await?;

    let users = find_many::<User>().includes(|x| x.tokens()).execute(&client).await?;
    assert_eq!(users.len(), 1);
    assert_eq!(users[0].tokens.len(), 3);
    assert!(users[0].tokens.iter().all(|token| token.user_id.as_deref() == Some(users[0].id.as_str())));

    let _ = std::fs::remove_file(path);
    Ok(())
}

#[tokio::test]
async fn implicit_many_to_many_uses_virtual_keys_for_insert_update_and_includes() -> anyhow::Result<()> {
    let (client, path) = client("many-to-many-relations").await?;
    let Backend::Sqlite(adapter) = &client.backend else { unreachable!("sqlite test") };
    create_table(
        adapter,
        "relation_business",
        vec![primary(column("id", MigrationColumnType::String)), column("name", MigrationColumnType::String)],
    )
    .await?;
    create_table(
        adapter,
        "relation_system",
        vec![primary(column("id", MigrationColumnType::String)), column("name", MigrationColumnType::String)],
    )
    .await?;
    create_table(
        adapter,
        "_relation_business_to_relation_system",
        vec![column("business_id", MigrationColumnType::String), column("system_id", MigrationColumnType::String)],
    )
    .await?;

    let business = Business::new("business-a".to_string(), "Dinoco".to_string());
    let system = System::new("system-a".to_string(), "Backoffice".to_string());
    insert_into::<Business>().values(&business).execute(&client).await?;
    insert_into::<System>().values(&system).execute(&client).await?;

    let updated = find_and_update::<Business>()
        .where_(|item| item.id.eq(&business.id))
        .update(|item| item.system_id.connect(&system.id))
        .execute(&client)
        .await?;
    assert_eq!(updated.id, business.id);
    assert!(updated.system_id.is_none());

    let loaded = find_many::<Business>()
        .includes(|item| item.systems().includes(|system| system.businesses()))
        .execute(&client)
        .await?;
    assert_eq!(loaded[0].systems.len(), 1);
    assert_eq!(loaded[0].systems[0].businesses.len(), 1);
    assert!(loaded[0].system_id.is_none());
    assert!(loaded[0].systems[0].business_id.is_none());

    let relation_count = count::<Business>().includes(|item| item.systems()).execute(&client).await?;
    assert_eq!(relation_count.systems, Some(1));

    dinoco::update::<Business>()
        .where_(|item| item.id.eq(&business.id))
        .update(|item| item.system_id.disconnect(&system.id))
        .execute(&client)
        .await?;
    let loaded = find_many::<Business>().includes(|item| item.systems()).execute(&client).await?;
    assert!(loaded[0].systems.is_empty());

    let mut inserted_business = Business::new("business-b".to_string(), "Dinoco Docs".to_string());
    inserted_business.system_id = Some(system.id.clone());
    insert_into::<Business>().values(&inserted_business).execute(&client).await?;

    let loaded_systems = find_many::<System>()
        .where_(|item| item.id.eq(&system.id))
        .includes(|item| item.businesses())
        .execute(&client)
        .await?;
    assert_eq!(loaded_systems[0].businesses.len(), 1);
    assert_eq!(loaded_systems[0].businesses[0].id, inserted_business.id);
    assert!(loaded_systems[0].business_id.is_none());

    let mut inserted_system = System::new("system-b".to_string(), "ERP".to_string());
    inserted_system.business_id = Some(business.id.clone());
    insert_into::<System>().values(&inserted_system).execute(&client).await?;

    let mut inserted_systems = vec![
        System::new("system-c".to_string(), "CRM".to_string()),
        System::new("system-d".to_string(), "Analytics".to_string()),
    ];
    for system in &mut inserted_systems {
        system.business_id = Some(business.id.clone());
    }
    insert_many::<System>().values(&inserted_systems).execute(&client).await?;

    let loaded = find_many::<Business>().includes(|item| item.systems()).execute(&client).await?;
    let mut system_ids = loaded[0].systems.iter().map(|system| system.id.as_str()).collect::<Vec<_>>();
    system_ids.sort_unstable();
    assert_eq!(system_ids, ["system-b", "system-c", "system-d"]);
    assert!(loaded[0].systems.iter().all(|system| system.business_id.is_none()));

    let _ = std::fs::remove_file(path);
    Ok(())
}

async fn client(name: &str) -> anyhow::Result<(DinocoClient, String)> {
    let path = format!("/private/tmp/dinoco-{name}-{}-{}.sqlite", std::process::id(), monotonic());
    let adapter = SqliteAdapter::new(path.clone()).await.map_err(anyhow::Error::msg)?;
    Ok((DinocoClient::new(Backend::Sqlite(adapter)), path))
}

fn monotonic() -> u128 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
}
