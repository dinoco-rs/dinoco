//! Regression coverage for the reported "sibling includes interfere with each
//! other" bug: a direct `one_to_many` relation (`Business.offices`) must stay
//! populated even when a sibling nested `many_to_one` relation reachable
//! through a different parent (`Business.access[*].office`) targets the same
//! underlying table.

use dinoco::{Entity, find_first, find_many, insert_into};
use dinoco_engine::{Backend, DinocoAdapter, DinocoClient, MigrationColumnType, SqliteAdapter};
use dinoco_tests::{column, create_table, nullable, primary};

#[derive(Debug, Entity)]
#[dinoco(table_name = "complex_business")]
struct Business {
    #[dinoco(primary_key)]
    id: String,
    name: String,

    #[dinoco(one_to_many, foreign_key = "business_id", references = "id")]
    offices: Vec<Office>,

    #[dinoco(one_to_many, foreign_key = "business_id", references = "id")]
    access: Vec<Access>,
}

#[derive(Debug, Entity)]
#[dinoco(table_name = "complex_office")]
struct Office {
    #[dinoco(primary_key)]
    id: String,
    business_id: Option<String>,
    name: String,

    #[dinoco(many_to_one, foreign_key = "business_id", references = "id")]
    business: Option<Business>,

    #[dinoco(one_to_many, foreign_key = "office_id", references = "id")]
    permissions: Vec<Permission>,
}

#[derive(Debug, Entity)]
#[dinoco(table_name = "complex_permission")]
struct Permission {
    #[dinoco(primary_key)]
    id: String,
    office_id: Option<String>,
    label: String,

    #[dinoco(many_to_one, foreign_key = "office_id", references = "id")]
    office: Option<Office>,
}

#[derive(Debug, Entity)]
#[dinoco(table_name = "complex_access")]
struct Access {
    #[dinoco(primary_key)]
    id: String,
    business_id: Option<String>,
    office_id: Option<String>,

    #[dinoco(many_to_one, foreign_key = "business_id", references = "id")]
    business: Option<Business>,

    #[dinoco(many_to_one, foreign_key = "office_id", references = "id")]
    office: Option<Office>,
}

#[tokio::test]
async fn sibling_one_to_many_and_nested_belongs_to_do_not_interfere() -> anyhow::Result<()> {
    let (client, path) = client("complex-relations-sibling").await?;
    let Backend::Sqlite(adapter) = &client.backend else { unreachable!("sqlite test") };

    create_table(
        adapter,
        "complex_business",
        vec![primary(column("id", MigrationColumnType::String)), column("name", MigrationColumnType::String)],
    )
    .await?;
    create_table(
        adapter,
        "complex_office",
        vec![
            primary(column("id", MigrationColumnType::String)),
            nullable(column("business_id", MigrationColumnType::String)),
            column("name", MigrationColumnType::String),
        ],
    )
    .await?;
    create_table(
        adapter,
        "complex_permission",
        vec![
            primary(column("id", MigrationColumnType::String)),
            nullable(column("office_id", MigrationColumnType::String)),
            column("label", MigrationColumnType::String),
        ],
    )
    .await?;
    create_table(
        adapter,
        "complex_access",
        vec![
            primary(column("id", MigrationColumnType::String)),
            nullable(column("business_id", MigrationColumnType::String)),
            nullable(column("office_id", MigrationColumnType::String)),
        ],
    )
    .await?;

    let business = Business::new("business-a".to_string(), "Dinoco".to_string());
    insert_into::<Business>().values(&business).execute(&client).await?;

    let mut office_a = Office::new("office-a".to_string(), "Headquarters".to_string());
    office_a.business_id = Some(business.id.clone());
    insert_into::<Office>().values(&office_a).execute(&client).await?;

    let mut office_b = Office::new("office-b".to_string(), "Branch".to_string());
    office_b.business_id = Some(business.id.clone());
    insert_into::<Office>().values(&office_b).execute(&client).await?;

    let mut permission = Permission::new("permission-a".to_string(), "read".to_string());
    permission.office_id = Some(office_a.id.clone());
    insert_into::<Permission>().values(&permission).execute(&client).await?;

    // `access` points back at `office_a` through a completely different
    // relation (`Access.office`, many_to_one) than `Business.offices`
    // (one_to_many). Both ultimately read from the same `complex_office`
    // table for the same row.
    let mut access = Access::new("access-a".to_string());
    access.business_id = Some(business.id.clone());
    access.office_id = Some(office_a.id.clone());
    insert_into::<Access>().values(&access).execute(&client).await?;

    let business_id = business.id.clone();
    let loaded = find_first::<Business>()
        .where_(|item| item.id.eq(&business_id))
        .includes(|item| item.access().includes(|access| access.office()))
        .includes(|item| item.offices().includes(|office| office.permissions()))
        .execute(&client)
        .await?
        .expect("business");

    assert_eq!(loaded.access.len(), 1);
    assert_eq!(loaded.offices.len(), 2);

    let mut office_ids = loaded.offices.iter().map(|office| office.id.as_str()).collect::<Vec<_>>();
    office_ids.sort_unstable();
    assert_eq!(office_ids, ["office-a", "office-b"]);

    let accessed_office = loaded.access[0].office.as_ref().expect("access.office include");
    assert_eq!(accessed_office.id, office_a.id);
    assert!(loaded.offices.iter().any(|office| office.id == accessed_office.id));

    let loaded_office_a = loaded.offices.iter().find(|office| office.id == office_a.id).expect("office-a in offices");
    assert_eq!(loaded_office_a.permissions.len(), 1);
    assert_eq!(loaded_office_a.permissions[0].id, permission.id);

    let loaded_office_b = loaded.offices.iter().find(|office| office.id == office_b.id).expect("office-b in offices");
    assert!(loaded_office_b.permissions.is_empty());

    // `find_many` must show the same behavior as `find_first`.
    let loaded_many = find_many::<Business>()
        .includes(|item| item.access().includes(|access| access.office()))
        .includes(|item| item.offices().includes(|office| office.permissions()))
        .execute(&client)
        .await?;
    assert_eq!(loaded_many.len(), 1);
    assert_eq!(loaded_many[0].offices.len(), 2);
    assert_eq!(loaded_many[0].access.len(), 1);
    assert_eq!(loaded_many[0].access[0].office.as_ref().map(|office| office.id.as_str()), Some(office_a.id.as_str()));

    // Swapping the include order must not change the outcome.
    let loaded_swapped = find_first::<Business>()
        .where_(|item| item.id.eq(&business_id))
        .includes(|item| item.offices().includes(|office| office.permissions()))
        .includes(|item| item.access().includes(|access| access.office()))
        .execute(&client)
        .await?
        .expect("business (swapped include order)");
    assert_eq!(loaded_swapped.offices.len(), 2);
    assert_eq!(loaded_swapped.access.len(), 1);

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
