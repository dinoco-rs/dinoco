//! Regression coverage for the reported "sibling includes interfere with each
//! other" bug at realistic scale: the exact relation tree from the task brief
//! (nested many-to-many, self-relations, multiple relations to the same
//! target model through different paths, and deep nested includes all in one
//! query). `business.offices` must stay populated even though
//! `business.access[*].office` targets the exact same table, and every other
//! sibling relation must keep its own data.

use dinoco::{Entity, find_first, find_many, insert_into};
use dinoco_engine::{Backend, DinocoAdapter, DinocoClient, MigrationColumnType, SqliteAdapter};
use dinoco_tests::{column, create_table, nullable, primary};

#[derive(Debug, Entity)]
#[dinoco(table_name = "tree_business")]
struct Business {
    #[dinoco(primary_key)]
    id: String,
    name: String,

    #[dinoco(one_to_many, foreign_key = "business_id", references = "id")]
    documents: Vec<Document>,

    #[dinoco(one_to_many, foreign_key = "business_id", references = "id")]
    reviews: Vec<Review>,

    #[dinoco(
        many_to_many,
        foreign_key = "id",
        references = "id",
        join_table = "_tree_business_to_tree_cnae",
        join_parent_field = "business_id",
        join_child_field = "cnae_id"
    )]
    cnaes: Vec<Cnae>,

    #[dinoco(
        many_to_many,
        foreign_key = "id",
        references = "id",
        join_table = "_tree_business_to_tree_partner",
        join_parent_field = "business_id",
        join_child_field = "partner_id"
    )]
    partners: Vec<Partner>,

    #[dinoco(one_to_many, foreign_key = "business_id", references = "id")]
    fees: Vec<Fee>,

    #[dinoco(one_to_many, foreign_key = "business_id", references = "id")]
    offices: Vec<Office>,

    #[dinoco(one_to_many, foreign_key = "business_id", references = "id")]
    access: Vec<Access>,
}

#[derive(Debug, Entity)]
#[dinoco(table_name = "tree_cdn_file")]
struct CdnFile {
    #[dinoco(primary_key)]
    id: String,
    url: String,
}

#[derive(Debug, Entity)]
#[dinoco(table_name = "tree_document")]
struct Document {
    #[dinoco(primary_key)]
    id: String,
    business_id: Option<String>,
    review_data_id: Option<String>,
    cdn_file_id: Option<String>,
    name: String,

    #[dinoco(many_to_one, foreign_key = "business_id", references = "id")]
    business: Option<Business>,

    #[dinoco(many_to_one, foreign_key = "review_data_id", references = "id")]
    review_data: Option<ReviewData>,

    #[dinoco(many_to_one, foreign_key = "cdn_file_id", references = "id")]
    cdn_file: Option<CdnFile>,
}

#[derive(Debug, Entity)]
#[dinoco(table_name = "tree_review_data")]
struct ReviewData {
    #[dinoco(primary_key)]
    id: String,

    #[dinoco(one_to_many, foreign_key = "review_data_id", references = "id")]
    documents: Vec<Document>,
}

#[derive(Debug, Entity)]
#[dinoco(table_name = "tree_review")]
struct Review {
    #[dinoco(primary_key)]
    id: String,
    business_id: Option<String>,
    new_data_id: Option<String>,
    old_data_id: Option<String>,

    #[dinoco(many_to_one, foreign_key = "business_id", references = "id")]
    business: Option<Business>,

    #[dinoco(many_to_one, relation_name = "new_data", foreign_key = "new_data_id", references = "id")]
    new_data: Option<ReviewData>,

    #[dinoco(many_to_one, relation_name = "old_data", foreign_key = "old_data_id", references = "id")]
    old_data: Option<ReviewData>,
}

#[derive(Debug, Entity)]
#[dinoco(table_name = "tree_cnae")]
struct Cnae {
    #[dinoco(primary_key)]
    id: String,
    code: String,
}

#[derive(Debug, Entity)]
#[dinoco(table_name = "tree_partner")]
struct Partner {
    #[dinoco(primary_key)]
    id: String,
    name: String,
}

#[derive(Debug, Entity)]
#[dinoco(table_name = "tree_fee")]
struct Fee {
    #[dinoco(primary_key)]
    id: String,
    business_id: Option<String>,
    parent_fee_id: Option<String>,
    label: String,

    #[dinoco(many_to_one, foreign_key = "business_id", references = "id")]
    business: Option<Business>,

    #[dinoco(many_to_one, foreign_key = "parent_fee_id", references = "id")]
    parent: Option<Box<Fee>>,

    #[dinoco(one_to_many, foreign_key = "parent_fee_id", references = "id")]
    fees: Vec<Fee>,

    #[dinoco(one_to_many, foreign_key = "fee_id", references = "id")]
    parts: Vec<FeePart>,
}

#[derive(Debug, Entity)]
#[dinoco(table_name = "tree_fee_part")]
struct FeePart {
    #[dinoco(primary_key)]
    id: String,
    fee_id: Option<String>,
    label: String,

    #[dinoco(many_to_one, foreign_key = "fee_id", references = "id")]
    fee: Option<Fee>,
}

#[derive(Debug, Entity)]
#[dinoco(table_name = "tree_office")]
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
#[dinoco(table_name = "tree_permission")]
struct Permission {
    #[dinoco(primary_key)]
    id: String,
    office_id: Option<String>,
    label: String,

    #[dinoco(many_to_one, foreign_key = "office_id", references = "id")]
    office: Option<Office>,
}

#[derive(Debug, Entity)]
#[dinoco(table_name = "tree_account")]
struct Account {
    #[dinoco(primary_key)]
    id: String,
    name: String,
}

#[derive(Debug, Entity)]
#[dinoco(table_name = "tree_access")]
struct Access {
    #[dinoco(primary_key)]
    id: String,
    business_id: Option<String>,
    sent_by_id: Option<String>,
    office_id: Option<String>,
    account_id: Option<String>,

    #[dinoco(many_to_one, foreign_key = "business_id", references = "id")]
    business: Option<Business>,

    #[dinoco(many_to_one, foreign_key = "sent_by_id", references = "id")]
    sent_by: Option<Account>,

    #[dinoco(many_to_one, foreign_key = "office_id", references = "id")]
    office: Option<Office>,

    #[dinoco(many_to_one, foreign_key = "account_id", references = "id")]
    account: Option<Account>,
}

#[tokio::test]
async fn deep_business_tree_keeps_every_sibling_relation_independent() -> anyhow::Result<()> {
    let (client, path) = client("complex-business-tree").await?;
    let Backend::Sqlite(adapter) = &client.backend else { unreachable!("sqlite test") };

    create_table(
        adapter,
        "tree_business",
        vec![primary(column("id", MigrationColumnType::String)), column("name", MigrationColumnType::String)],
    )
    .await?;
    create_table(
        adapter,
        "tree_cdn_file",
        vec![primary(column("id", MigrationColumnType::String)), column("url", MigrationColumnType::String)],
    )
    .await?;
    create_table(
        adapter,
        "tree_document",
        vec![
            primary(column("id", MigrationColumnType::String)),
            nullable(column("business_id", MigrationColumnType::String)),
            nullable(column("review_data_id", MigrationColumnType::String)),
            nullable(column("cdn_file_id", MigrationColumnType::String)),
            column("name", MigrationColumnType::String),
        ],
    )
    .await?;
    create_table(adapter, "tree_review_data", vec![primary(column("id", MigrationColumnType::String))]).await?;
    create_table(
        adapter,
        "tree_review",
        vec![
            primary(column("id", MigrationColumnType::String)),
            nullable(column("business_id", MigrationColumnType::String)),
            nullable(column("new_data_id", MigrationColumnType::String)),
            nullable(column("old_data_id", MigrationColumnType::String)),
        ],
    )
    .await?;
    create_table(
        adapter,
        "tree_cnae",
        vec![primary(column("id", MigrationColumnType::String)), column("code", MigrationColumnType::String)],
    )
    .await?;
    create_table(
        adapter,
        "tree_partner",
        vec![primary(column("id", MigrationColumnType::String)), column("name", MigrationColumnType::String)],
    )
    .await?;
    create_table(
        adapter,
        "_tree_business_to_tree_cnae",
        vec![column("business_id", MigrationColumnType::String), column("cnae_id", MigrationColumnType::String)],
    )
    .await?;
    create_table(
        adapter,
        "_tree_business_to_tree_partner",
        vec![column("business_id", MigrationColumnType::String), column("partner_id", MigrationColumnType::String)],
    )
    .await?;
    create_table(
        adapter,
        "tree_fee",
        vec![
            primary(column("id", MigrationColumnType::String)),
            nullable(column("business_id", MigrationColumnType::String)),
            nullable(column("parent_fee_id", MigrationColumnType::String)),
            column("label", MigrationColumnType::String),
        ],
    )
    .await?;
    create_table(
        adapter,
        "tree_fee_part",
        vec![
            primary(column("id", MigrationColumnType::String)),
            nullable(column("fee_id", MigrationColumnType::String)),
            column("label", MigrationColumnType::String),
        ],
    )
    .await?;
    create_table(
        adapter,
        "tree_office",
        vec![
            primary(column("id", MigrationColumnType::String)),
            nullable(column("business_id", MigrationColumnType::String)),
            column("name", MigrationColumnType::String),
        ],
    )
    .await?;
    create_table(
        adapter,
        "tree_permission",
        vec![
            primary(column("id", MigrationColumnType::String)),
            nullable(column("office_id", MigrationColumnType::String)),
            column("label", MigrationColumnType::String),
        ],
    )
    .await?;
    create_table(
        adapter,
        "tree_account",
        vec![primary(column("id", MigrationColumnType::String)), column("name", MigrationColumnType::String)],
    )
    .await?;
    create_table(
        adapter,
        "tree_access",
        vec![
            primary(column("id", MigrationColumnType::String)),
            nullable(column("business_id", MigrationColumnType::String)),
            nullable(column("sent_by_id", MigrationColumnType::String)),
            nullable(column("office_id", MigrationColumnType::String)),
            nullable(column("account_id", MigrationColumnType::String)),
        ],
    )
    .await?;

    let business = Business::new("business-a".to_string(), "Dinoco".to_string());
    insert_into::<Business>().values(&business).execute(&client).await?;

    // --- documents directly on the business, each with its own CDN file ---
    let cdn_a = CdnFile::new("cdn-a".to_string(), "https://cdn.example/a".to_string());
    let cdn_b = CdnFile::new("cdn-b".to_string(), "https://cdn.example/b".to_string());
    insert_into::<CdnFile>().values(&cdn_a).execute(&client).await?;
    insert_into::<CdnFile>().values(&cdn_b).execute(&client).await?;

    let mut document_a = Document::new("document-a".to_string(), "Contract".to_string());
    document_a.business_id = Some(business.id.clone());
    document_a.cdn_file_id = Some(cdn_a.id.clone());
    insert_into::<Document>().values(&document_a).execute(&client).await?;

    let mut document_b = Document::new("document-b".to_string(), "Invoice".to_string());
    document_b.business_id = Some(business.id.clone());
    document_b.cdn_file_id = Some(cdn_b.id.clone());
    insert_into::<Document>().values(&document_b).execute(&client).await?;

    // --- a review with new/old data, each carrying its own documents ---
    let new_data = ReviewData::new("review-data-new".to_string());
    let old_data = ReviewData::new("review-data-old".to_string());
    insert_into::<ReviewData>().values(&new_data).execute(&client).await?;
    insert_into::<ReviewData>().values(&old_data).execute(&client).await?;

    let cdn_new = CdnFile::new("cdn-new".to_string(), "https://cdn.example/new".to_string());
    let cdn_old = CdnFile::new("cdn-old".to_string(), "https://cdn.example/old".to_string());
    insert_into::<CdnFile>().values(&cdn_new).execute(&client).await?;
    insert_into::<CdnFile>().values(&cdn_old).execute(&client).await?;

    let mut new_document = Document::new("document-new".to_string(), "New scan".to_string());
    new_document.review_data_id = Some(new_data.id.clone());
    new_document.cdn_file_id = Some(cdn_new.id.clone());
    insert_into::<Document>().values(&new_document).execute(&client).await?;

    let mut old_document = Document::new("document-old".to_string(), "Old scan".to_string());
    old_document.review_data_id = Some(old_data.id.clone());
    old_document.cdn_file_id = Some(cdn_old.id.clone());
    insert_into::<Document>().values(&old_document).execute(&client).await?;

    let mut review = Review::new("review-a".to_string());
    review.business_id = Some(business.id.clone());
    review.new_data_id = Some(new_data.id.clone());
    review.old_data_id = Some(old_data.id.clone());
    insert_into::<Review>().values(&review).execute(&client).await?;

    // --- many-to-many cnaes and partners ---
    let cnae_a = Cnae::new("cnae-a".to_string(), "6201-5".to_string());
    let cnae_b = Cnae::new("cnae-b".to_string(), "6202-3".to_string());
    insert_into::<Cnae>().values(&cnae_a).execute(&client).await?;
    insert_into::<Cnae>().values(&cnae_b).execute(&client).await?;
    for cnae_id in [&cnae_a.id, &cnae_b.id] {
        adapter
            .execute(
                "INSERT INTO _tree_business_to_tree_cnae (business_id, cnae_id) VALUES (?1, ?2)",
                &[business.id.clone().into(), cnae_id.clone().into()],
            )
            .await?;
    }

    let partner_a = Partner::new("partner-a".to_string(), "Alice".to_string());
    insert_into::<Partner>().values(&partner_a).execute(&client).await?;
    adapter
        .execute(
            "INSERT INTO _tree_business_to_tree_partner (business_id, partner_id) VALUES (?1, ?2)",
            &[business.id.clone().into(), partner_a.id.clone().into()],
        )
        .await?;

    // --- self-relation fees: fee-a has one nested fee with one part ---
    let mut fee_a = Fee::new("fee-a".to_string(), "Base fee".to_string());
    fee_a.business_id = Some(business.id.clone());
    insert_into::<Fee>().values(&fee_a).execute(&client).await?;

    let mut fee_a_child = Fee::new("fee-a-child".to_string(), "Surcharge".to_string());
    fee_a_child.parent_fee_id = Some(fee_a.id.clone());
    insert_into::<Fee>().values(&fee_a_child).execute(&client).await?;

    let mut fee_part = FeePart::new("fee-part-a".to_string(), "Tax".to_string());
    fee_part.fee_id = Some(fee_a_child.id.clone());
    insert_into::<FeePart>().values(&fee_part).execute(&client).await?;

    let mut fee_b = Fee::new("fee-b".to_string(), "Optional fee".to_string());
    fee_b.business_id = Some(business.id.clone());
    insert_into::<Fee>().values(&fee_b).execute(&client).await?;

    // --- offices with permissions ---
    let mut office_a = Office::new("office-a".to_string(), "Headquarters".to_string());
    office_a.business_id = Some(business.id.clone());
    insert_into::<Office>().values(&office_a).execute(&client).await?;

    let mut office_b = Office::new("office-b".to_string(), "Branch".to_string());
    office_b.business_id = Some(business.id.clone());
    insert_into::<Office>().values(&office_b).execute(&client).await?;

    let mut permission = Permission::new("permission-a".to_string(), "read".to_string());
    permission.office_id = Some(office_a.id.clone());
    insert_into::<Permission>().values(&permission).execute(&client).await?;

    // --- access, pointing back at office-a through a *different* relation
    //     path than `business.offices` ---
    let sender = Account::new("account-sender".to_string(), "Sender".to_string());
    let holder = Account::new("account-holder".to_string(), "Holder".to_string());
    insert_into::<Account>().values(&sender).execute(&client).await?;
    insert_into::<Account>().values(&holder).execute(&client).await?;

    let mut access = Access::new("access-a".to_string());
    access.business_id = Some(business.id.clone());
    access.sent_by_id = Some(sender.id.clone());
    access.office_id = Some(office_a.id.clone());
    access.account_id = Some(holder.id.clone());
    insert_into::<Access>().values(&access).execute(&client).await?;

    let business_id = business.id.clone();
    let loaded = find_first::<Business>()
        .where_(|item| item.id.eq(&business_id))
        .includes(|item| {
            item.reviews()
                .includes(|review| review.new_data().includes(|data| data.documents().includes(|doc| doc.cdn_file())))
                .includes(|review| review.old_data().includes(|data| data.documents().includes(|doc| doc.cdn_file())))
        })
        .includes(|item| item.documents().includes(|doc| doc.cdn_file()))
        .includes(|item| item.cnaes())
        .includes(|item| item.partners())
        .includes(|item| item.fees().includes(|fee| fee.fees().includes(|child| child.parts())))
        .includes(|item| item.offices().includes(|office| office.permissions()))
        .includes(|item| item.access().includes(|access| access.sent_by()).includes(|access| access.office()).includes(|access| access.account()))
        .execute(&client)
        .await?
        .expect("business");

    // The core regression: `offices` must stay fully populated even though
    // `access[*].office` reaches the exact same `office-a` row.
    assert_eq!(loaded.offices.len(), 2);
    assert_eq!(loaded.access.len(), 1);
    assert!(loaded.offices.iter().any(|office| office.id == office_a.id));
    assert!(loaded.offices.iter().any(|office| office.id == office_b.id));
    assert_eq!(loaded.access[0].office.as_ref().unwrap().id, office_a.id);

    let loaded_office_a = loaded.offices.iter().find(|office| office.id == office_a.id).expect("office-a");
    assert_eq!(loaded_office_a.permissions.len(), 1);
    assert_eq!(loaded_office_a.permissions[0].id, permission.id);
    let loaded_office_b = loaded.offices.iter().find(|office| office.id == office_b.id).expect("office-b");
    assert!(loaded_office_b.permissions.is_empty());

    assert_eq!(loaded.access[0].sent_by.as_ref().map(|account| account.id.as_str()), Some(sender.id.as_str()));
    assert_eq!(loaded.access[0].account.as_ref().map(|account| account.id.as_str()), Some(holder.id.as_str()));

    // documents + cdn_file, unaffected by reviews/access loading the same tables.
    assert_eq!(loaded.documents.len(), 2);
    let loaded_document_a = loaded.documents.iter().find(|doc| doc.id == document_a.id).expect("document-a");
    assert_eq!(loaded_document_a.cdn_file.as_ref().map(|cdn| cdn.id.as_str()), Some(cdn_a.id.as_str()));
    let loaded_document_b = loaded.documents.iter().find(|doc| doc.id == document_b.id).expect("document-b");
    assert_eq!(loaded_document_b.cdn_file.as_ref().map(|cdn| cdn.id.as_str()), Some(cdn_b.id.as_str()));

    // reviews -> new_data/old_data -> documents -> cdn_file, both sides distinct.
    assert_eq!(loaded.reviews.len(), 1);
    let loaded_review = &loaded.reviews[0];
    let loaded_new_data = loaded_review.new_data.as_ref().expect("new_data include");
    assert_eq!(loaded_new_data.id, new_data.id);
    assert_eq!(loaded_new_data.documents.len(), 1);
    assert_eq!(loaded_new_data.documents[0].id, new_document.id);
    assert_eq!(loaded_new_data.documents[0].cdn_file.as_ref().map(|cdn| cdn.id.as_str()), Some(cdn_new.id.as_str()));

    let loaded_old_data = loaded_review.old_data.as_ref().expect("old_data include");
    assert_eq!(loaded_old_data.id, old_data.id);
    assert_eq!(loaded_old_data.documents.len(), 1);
    assert_eq!(loaded_old_data.documents[0].id, old_document.id);
    assert_eq!(loaded_old_data.documents[0].cdn_file.as_ref().map(|cdn| cdn.id.as_str()), Some(cdn_old.id.as_str()));

    // many-to-many cnaes/partners, independent of each other.
    let mut cnae_ids = loaded.cnaes.iter().map(|cnae| cnae.id.as_str()).collect::<Vec<_>>();
    cnae_ids.sort_unstable();
    assert_eq!(cnae_ids, ["cnae-a", "cnae-b"]);
    assert_eq!(loaded.partners.len(), 1);
    assert_eq!(loaded.partners[0].id, partner_a.id);

    // self-relation fees -> fees -> parts.
    assert_eq!(loaded.fees.len(), 2);
    let loaded_fee_a = loaded.fees.iter().find(|fee| fee.id == fee_a.id).expect("fee-a");
    assert_eq!(loaded_fee_a.fees.len(), 1);
    assert_eq!(loaded_fee_a.fees[0].id, fee_a_child.id);
    assert_eq!(loaded_fee_a.fees[0].parts.len(), 1);
    assert_eq!(loaded_fee_a.fees[0].parts[0].id, fee_part.id);
    let loaded_fee_b = loaded.fees.iter().find(|fee| fee.id == fee_b.id).expect("fee-b");
    assert!(loaded_fee_b.fees.is_empty());

    // `find_many` must behave identically to `find_first` for the same query.
    let loaded_many = find_many::<Business>()
        .includes(|item| item.offices().includes(|office| office.permissions()))
        .includes(|item| item.access().includes(|access| access.office()))
        .execute(&client)
        .await?;
    assert_eq!(loaded_many.len(), 1);
    assert_eq!(loaded_many[0].offices.len(), 2);
    assert_eq!(loaded_many[0].access.len(), 1);
    assert_eq!(loaded_many[0].access[0].office.as_ref().map(|office| office.id.as_str()), Some(office_a.id.as_str()));

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
