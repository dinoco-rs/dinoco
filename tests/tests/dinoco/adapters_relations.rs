use dinoco::{DinocoEnum, Entity, count, delete, find_and_update, find_first, find_many, insert_into, insert_many};
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

    #[dinoco(one_to_many, foreign_key = "business_id", references = "id")]
    access: Vec<SystemAccess>,

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
    category_id: Option<String>,

    #[dinoco(many_to_one, foreign_key = "category_id", references = "id")]
    category: Option<SystemCategory>,

    #[dinoco(one_to_many, foreign_key = "system_id", references = "id")]
    notes: Vec<SystemNote>,

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

#[derive(Debug, Entity)]
#[dinoco(table_name = "relation_system_category")]
pub struct SystemCategory {
    #[dinoco(primary_key)]
    id: String,
    name: String,
}

#[derive(Debug, Entity)]
#[dinoco(table_name = "relation_system_note")]
pub struct SystemNote {
    #[dinoco(primary_key)]
    id: String,
    system_id: Option<String>,
    body: String,

    #[dinoco(many_to_one, foreign_key = "system_id", references = "id")]
    system: Option<System>,
}

#[derive(Debug, Entity)]
#[dinoco(table_name = "relation_system_access")]
pub struct SystemAccess {
    #[dinoco(primary_key)]
    id: String,
    business_id: Option<String>,
    system_id: Option<String>,

    #[dinoco(many_to_one, foreign_key = "business_id", references = "id")]
    business: Option<Business>,

    #[dinoco(many_to_one, foreign_key = "system_id", references = "id")]
    system: Option<System>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, DinocoEnum)]
enum PassState {
    #[default]
    #[dinoco(value = "enabled")]
    Enabled,
    #[dinoco(value = "waiting")]
    Waiting,
}

#[derive(Debug, Entity)]
#[dinoco(table_name = "named_relation_patron")]
struct Patron {
    #[dinoco(primary_key)]
    id: String,

    #[dinoco(one_to_many, foreign_key = "curator_id", references = "id")]
    studio: Vec<Studio>,

    #[dinoco(one_to_many, relation_name = "holder", foreign_key = "patron_id", references = "id")]
    borrowed_passes: Vec<VenuePass>,

    #[dinoco(one_to_many, relation_name = "issued_by", foreign_key = "issued_by_id", references = "id")]
    issued_passes: Vec<VenuePass>,
}

#[derive(Debug, Entity)]
#[dinoco(table_name = "named_relation_signin")]
struct Signin {
    #[dinoco(primary_key)]
    id: String,
    enabled: bool,
    patron_id: Option<String>,

    #[dinoco(many_to_one, foreign_key = "patron_id", references = "id")]
    patron: Option<Patron>,
}

#[derive(Debug, Entity)]
#[dinoco(table_name = "named_relation_studio")]
struct Studio {
    #[dinoco(primary_key)]
    id: String,
    curator_id: Option<String>,

    #[dinoco(many_to_one, foreign_key = "curator_id", references = "id")]
    curator: Option<Patron>,
}

#[derive(Debug, Entity)]
#[dinoco(table_name = "named_relation_room")]
struct Room {
    #[dinoco(primary_key)]
    id: String,
}

#[derive(Debug, Entity)]
#[dinoco(table_name = "named_relation_venue_pass")]
struct VenuePass {
    #[dinoco(primary_key)]
    id: String,
    state: PassState,
    patron_id: Option<String>,

    #[dinoco(many_to_one, relation_name = "holder", foreign_key = "patron_id", references = "id")]
    patron: Option<Patron>,

    issued_by_id: Option<String>,

    #[dinoco(many_to_one, relation_name = "issued_by", foreign_key = "issued_by_id", references = "id")]
    issued_by: Option<Patron>,

    studio_id: Option<String>,

    #[dinoco(many_to_one, foreign_key = "studio_id", references = "id")]
    studio: Option<Studio>,

    room_id: Option<String>,

    #[dinoco(many_to_one, foreign_key = "room_id", references = "id")]
    room: Option<Room>,
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
        vec![
            primary(column("id", MigrationColumnType::String)),
            column("name", MigrationColumnType::String),
            nullable(column("category_id", MigrationColumnType::String)),
        ],
    )
    .await?;
    create_table(
        adapter,
        "relation_system_category",
        vec![primary(column("id", MigrationColumnType::String)), column("name", MigrationColumnType::String)],
    )
    .await?;
    create_table(
        adapter,
        "relation_system_note",
        vec![
            primary(column("id", MigrationColumnType::String)),
            nullable(column("system_id", MigrationColumnType::String)),
            column("body", MigrationColumnType::String),
        ],
    )
    .await?;
    create_table(
        adapter,
        "_relation_business_to_relation_system",
        vec![column("business_id", MigrationColumnType::String), column("system_id", MigrationColumnType::String)],
    )
    .await?;
    create_table(
        adapter,
        "relation_system_access",
        vec![
            primary(column("id", MigrationColumnType::String)),
            nullable(column("business_id", MigrationColumnType::String)),
            nullable(column("system_id", MigrationColumnType::String)),
        ],
    )
    .await?;

    let business = Business::new("business-a".to_string(), "Dinoco".to_string());
    let category = SystemCategory::new("category-a".to_string(), "Operations".to_string());
    insert_into::<SystemCategory>().values(&category).execute(&client).await?;

    let mut system = System::new("system-a".to_string(), "Backoffice".to_string());
    system.category_id = Some(category.id.clone());
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
    inserted_system.category_id = Some(category.id.clone());
    inserted_system.business_id = Some(business.id.clone());
    insert_into::<System>().values(&inserted_system).execute(&client).await?;

    let mut inserted_systems = vec![
        System::new("system-c".to_string(), "CRM".to_string()),
        System::new("system-d".to_string(), "Analytics".to_string()),
    ];
    for system in &mut inserted_systems {
        system.business_id = Some(business.id.clone());
        system.category_id = Some(category.id.clone());
    }
    insert_many::<System>().values(&inserted_systems).execute(&client).await?;

    let mut access = SystemAccess::new("access-a".to_string());
    access.business_id = Some(business.id.clone());
    access.system_id = Some(inserted_system.id.clone());
    insert_into::<SystemAccess>().values(&access).execute(&client).await?;

    for system_id in ["system-b", "system-c", "system-d"] {
        let mut note = SystemNote::new(format!("note-{system_id}"), format!("Note for {system_id}"));
        note.system_id = Some(system_id.to_string());
        insert_into::<SystemNote>().values(&note).execute(&client).await?;
    }

    let loaded = find_many::<Business>().includes(|item| item.systems()).execute(&client).await?;
    let mut system_ids = loaded[0].systems.iter().map(|system| system.id.as_str()).collect::<Vec<_>>();
    system_ids.sort_unstable();
    assert_eq!(system_ids, ["system-b", "system-c", "system-d"]);
    assert!(loaded[0].systems.iter().all(|system| system.business_id.is_none()));

    let loaded = find_first::<Business>()
        .where_(|item| item.id.eq(&business.id))
        .includes(|item| item.access().includes(|access| access.system()))
        .includes(|item| item.systems())
        .execute(&client)
        .await?
        .expect("business");
    assert_eq!(loaded.access.len(), 1);
    assert_eq!(loaded.access[0].system.as_ref().map(|system| system.id.as_str()), Some("system-b"));
    assert!(loaded.systems.iter().any(|system| system.id == "system-b"));

    let loaded_systems = find_many::<System>()
        .includes(|item| {
            item.businesses().includes(|business| {
                business.systems().includes(|system| system.category()).includes(|system| system.notes())
            })
        })
        .execute(&client)
        .await?;

    for system_id in ["system-b", "system-c", "system-d"] {
        let loaded_system = loaded_systems.iter().find(|system| system.id == system_id).unwrap();
        assert_eq!(loaded_system.businesses.len(), 1);
        assert_eq!(loaded_system.businesses[0].systems.len(), 3);
        for nested_system in &loaded_system.businesses[0].systems {
            assert_eq!(nested_system.category.as_ref().map(|category| category.id.as_str()), Some("category-a"));
            assert_eq!(nested_system.notes.len(), 1);
            assert_eq!(nested_system.notes[0].system_id.as_deref(), Some(nested_system.id.as_str()));
        }
    }

    let _ = std::fs::remove_file(path);
    Ok(())
}

#[tokio::test]
async fn named_relation_loads_filtered_nested_includes_from_the_correct_foreign_key() -> anyhow::Result<()> {
    let (client, path) = client("named-relation-nested-includes").await?;
    let Backend::Sqlite(adapter) = &client.backend else { unreachable!("sqlite test") };
    create_table(adapter, "named_relation_patron", vec![primary(column("id", MigrationColumnType::String))]).await?;
    create_table(
        adapter,
        "named_relation_signin",
        vec![
            primary(column("id", MigrationColumnType::String)),
            column("enabled", MigrationColumnType::Boolean),
            nullable(column("patron_id", MigrationColumnType::String)),
        ],
    )
    .await?;
    create_table(
        adapter,
        "named_relation_studio",
        vec![
            primary(column("id", MigrationColumnType::String)),
            nullable(column("curator_id", MigrationColumnType::String)),
        ],
    )
    .await?;
    create_table(adapter, "named_relation_room", vec![primary(column("id", MigrationColumnType::String))]).await?;
    create_table(
        adapter,
        "named_relation_venue_pass",
        vec![
            primary(column("id", MigrationColumnType::String)),
            column(
                "state",
                MigrationColumnType::Enum {
                    name: "PassState".to_string(),
                    values: vec!["enabled".to_string(), "waiting".to_string()],
                },
            ),
            nullable(column("patron_id", MigrationColumnType::String)),
            nullable(column("issued_by_id", MigrationColumnType::String)),
            nullable(column("studio_id", MigrationColumnType::String)),
            nullable(column("room_id", MigrationColumnType::String)),
        ],
    )
    .await?;

    let holder = Patron::new("holder-a".to_string());
    let issuer = Patron::new("issuer-b".to_string());
    insert_into::<Patron>().values(&holder).execute(&client).await?;
    insert_into::<Patron>().values(&issuer).execute(&client).await?;

    let mut signin = Signin::new("signin-a".to_string(), true);
    signin.patron_id = Some(holder.id.clone());
    insert_into::<Signin>().values(&signin).execute(&client).await?;

    let mut studio = Studio::new("studio-a".to_string());
    studio.curator_id = Some(holder.id.clone());
    insert_into::<Studio>().values(&studio).execute(&client).await?;
    let room = Room::new("room-a".to_string());
    insert_into::<Room>().values(&room).execute(&client).await?;

    let mut active_pass = VenuePass::new("pass-active".to_string(), PassState::Enabled);
    active_pass.patron_id = Some(holder.id.clone());
    active_pass.issued_by_id = Some(issuer.id.clone());
    active_pass.studio_id = Some(studio.id.clone());
    active_pass.room_id = Some(room.id.clone());
    insert_into::<VenuePass>().values(&active_pass).execute(&client).await?;

    let mut second_active_pass = VenuePass::new("pass-active-2".to_string(), PassState::Enabled);
    second_active_pass.patron_id = Some(holder.id.clone());
    second_active_pass.issued_by_id = Some(issuer.id.clone());
    second_active_pass.studio_id = Some(studio.id.clone());
    second_active_pass.room_id = Some(room.id.clone());
    insert_into::<VenuePass>().values(&second_active_pass).execute(&client).await?;

    let mut waiting_pass = VenuePass::new("pass-waiting".to_string(), PassState::Waiting);
    waiting_pass.patron_id = Some(holder.id.clone());
    waiting_pass.issued_by_id = Some(issuer.id.clone());
    insert_into::<VenuePass>().values(&waiting_pass).execute(&client).await?;

    let signin_id = signin.id.clone();
    let loaded = find_first::<Signin>()
        .where_(|signin| signin.id.eq(&signin_id))
        .where_(|signin| signin.enabled.eq(true))
        .includes(|signin| {
            signin.patron().includes(|patron| patron.studio()).includes(|patron| {
                patron
                    .borrowed_passes()
                    .where_(|pass| pass.state.eq(PassState::Enabled))
                    .includes(|pass| pass.studio())
                    .includes(|pass| pass.room())
            })
        })
        .execute(&client)
        .await?
        .expect("signin");

    let patron = loaded.patron.expect("patron include");
    assert_eq!(patron.studio.len(), 1);
    assert_eq!(patron.borrowed_passes.len(), 2);
    assert!(
        patron
            .borrowed_passes
            .iter()
            .all(|pass| pass.studio.as_ref().map(|studio| studio.id.as_str()) == Some("studio-a"))
    );
    assert!(
        patron.borrowed_passes.iter().all(|pass| pass.room.as_ref().map(|room| room.id.as_str()) == Some("room-a"))
    );

    delete::<VenuePass>().where_(|pass| pass.id.eq(&active_pass.id)).execute(&client).await?;

    let loaded_after_delete = find_first::<Signin>()
        .where_(|signin| signin.id.eq(&signin_id))
        .where_(|signin| signin.enabled.eq(true))
        .includes(|signin| {
            signin.patron().includes(|patron| patron.studio()).includes(|patron| {
                patron
                    .borrowed_passes()
                    .where_(|pass| pass.state.eq(PassState::Enabled))
                    .includes(|pass| pass.studio())
                    .includes(|pass| pass.room())
            })
        })
        .execute(&client)
        .await?
        .expect("signin after delete");

    let patron = loaded_after_delete.patron.expect("patron include after delete");
    assert_eq!(patron.studio.len(), 1);
    assert_eq!(patron.borrowed_passes.len(), 1);
    assert_eq!(patron.borrowed_passes[0].id, second_active_pass.id);
    assert_eq!(patron.borrowed_passes[0].studio.as_ref().map(|studio| studio.id.as_str()), Some("studio-a"));
    assert_eq!(patron.borrowed_passes[0].room.as_ref().map(|room| room.id.as_str()), Some("room-a"));

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
