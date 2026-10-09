use std::sync::{Arc, Mutex};

use dinoco::chrono::{DateTime, NaiveDate, TimeZone, Utc};
use dinoco::{
    DinocoClient, DinocoEntity, Entity, TestAmbient, TransactionError, UpdatedAtField, find_and_update, find_first,
    find_many, insert_many, setup_test_methods, transaction, update, update_many,
};

#[derive(Debug, Clone, Entity)]
#[dinoco(table_name = "article")]
pub struct Article {
    #[dinoco(primary_key)]
    id: String,
    title: String,
    #[dinoco(default = 0)]
    views: i64,
    #[dinoco(default = ::dinoco::chrono::Utc::now())]
    created_at: DateTime<Utc>,
    #[dinoco(updated_at, default = ::dinoco::chrono::Utc::now())]
    updated_at: DateTime<Utc>,
    #[dinoco(updated_at, default = ::dinoco::chrono::Utc::now().date_naive())]
    touched_on: NaiveDate,
    #[dinoco(updated_at, default = ::core::option::Option::Some(::dinoco::chrono::Utc::now()))]
    edited_at: Option<DateTime<Utc>>,

    #[dinoco(
        many_to_many_key,
        join_table = "_article_to_tag",
        parent_field = "id",
        join_parent_field = "article_id",
        join_child_field = "tag_id"
    )]
    tag_id: Option<String>,

    #[dinoco(
        many_to_many,
        join_table = "_article_to_tag",
        parent_field = "id",
        join_parent_field = "article_id",
        join_child_field = "tag_id",
        foreign_key = "id",
        references = "id"
    )]
    tags: Vec<Tag>,
}

#[derive(Debug, Clone, Entity)]
#[dinoco(table_name = "tag")]
pub struct Tag {
    #[dinoco(primary_key)]
    id: String,
    label: String,

    #[dinoco(
        many_to_many_key,
        join_table = "_article_to_tag",
        parent_field = "id",
        join_parent_field = "tag_id",
        join_child_field = "article_id"
    )]
    article_id: Option<String>,

    #[dinoco(
        many_to_many,
        join_table = "_article_to_tag",
        parent_field = "id",
        join_parent_field = "tag_id",
        join_child_field = "article_id",
        foreign_key = "id",
        references = "id"
    )]
    articles: Vec<Article>,
}

const SCHEMA: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/updated_at/schema.dinoco");

fn past() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2001, 2, 3, 4, 5, 6).single().expect("valid timestamp")
}

fn past_day() -> NaiveDate {
    past().date_naive()
}

/// Seeds `ids` with `updated_at`/`touched_on` far in the past, so a refresh is
/// observable without sleeping.
async fn seeded(ids: &[&str]) -> anyhow::Result<DinocoClient> {
    let client = TestAmbient::new().schema(SCHEMA).create().await?;
    let articles = ids
        .iter()
        .map(|id| {
            let mut article = Article::new(id.to_string(), format!("title {id}"));
            article.created_at = past();
            article.updated_at = past();
            article.touched_on = past_day();
            article.edited_at = None;
            article
        })
        .collect::<Vec<_>>();
    insert_many::<Article>().values(articles).execute(&client).await?;

    Ok(client)
}

async fn article(client: &DinocoClient, id: &str) -> anyhow::Result<Article> {
    let id = id.to_string();
    Ok(find_first::<Article>().where_(|article| article.id.eq(id)).execute(client).await?.expect("article"))
}

fn assert_refreshed(article: &Article, before: DateTime<Utc>) {
    assert!(article.updated_at > past(), "updated_at was not refreshed: {article:?}");
    assert!(article.updated_at >= before - chrono_slack(), "updated_at is not the current time: {article:?}");
    assert!(article.updated_at <= Utc::now() + chrono_slack(), "updated_at is in the future: {article:?}");
    assert_eq!(article.touched_on, Utc::now().date_naive());
    assert_eq!(article.edited_at, Some(article.updated_at), "both columns take the same statement's clock");
    assert_eq!(article.created_at, past(), "created_at must not change");
}

fn assert_untouched(article: &Article) {
    assert_eq!(article.updated_at, past());
    assert_eq!(article.touched_on, past_day());
    assert_eq!(article.edited_at, None);
}

// SQLite's clock has millisecond precision; allow for truncation.
fn chrono_slack() -> dinoco::chrono::Duration {
    dinoco::chrono::Duration::milliseconds(5)
}

#[test]
fn entity_derive_exposes_updated_at_fields() {
    assert_eq!(
        Article::UPDATED_AT_FIELDS,
        &[
            UpdatedAtField::timestamp("updated_at"),
            UpdatedAtField::date("touched_on"),
            UpdatedAtField::timestamp("edited_at"),
        ]
    );
    assert!(Tag::UPDATED_AT_FIELDS.is_empty());
}

#[tokio::test]
async fn update_refreshes_updated_at_with_the_database_clock() -> anyhow::Result<()> {
    let client = seeded(&["a", "b"]).await?;
    let statements = Arc::new(Mutex::new(Vec::new()));
    let recorded = statements.clone();
    setup_test_methods::<Article>(&client).on_update(move |_, query, _| {
        recorded.lock().unwrap().push((query.sql.clone(), query.params.len()));
    });

    let before = Utc::now();
    update::<Article>()
        .where_(|article| article.id.eq("a"))
        .update(|article| article.title.set("new"))
        .execute(&client)
        .await?;

    let changed = article(&client, "a").await?;
    assert_eq!(changed.title, "new");
    assert_refreshed(&changed, before);
    assert_untouched(&article(&client, "b").await?);

    // The timestamp is computed by the database, not bound from Rust.
    let statements = statements.lock().unwrap().clone();
    let (sql, params) = statements.last().expect("update statement");
    assert!(sql.contains("updated_at = strftime('%Y-%m-%dT%H:%M:%f+00:00', 'now')"), "{sql}");
    assert!(sql.contains("touched_on = date('now')"), "{sql}");
    assert!(sql.contains("edited_at = strftime("), "{sql}");
    assert_eq!(*params, 2, "only the title and the id are bound: {sql}");

    Ok(())
}

#[tokio::test]
async fn update_many_refreshes_every_matched_row() -> anyhow::Result<()> {
    let client = seeded(&["a", "b", "c"]).await?;
    let before = Utc::now();

    update_many::<Article>()
        .where_(|article| article.id.batch(["a", "b"]))
        .update(|article| article.views.increment(1))
        .execute(&client)
        .await?;

    for id in ["a", "b"] {
        let changed = article(&client, id).await?;
        assert_eq!(changed.views, 1);
        assert_refreshed(&changed, before);
    }
    assert_untouched(&article(&client, "c").await?);

    // The refreshed values are ordinary RFC 3339 timestamps: they compare
    // against the ones Dinoco writes itself.
    let recent = find_many::<Article>().where_(|article| article.updated_at.gt(past())).execute(&client).await?;
    assert_eq!(recent.len(), 2);

    Ok(())
}

#[tokio::test]
async fn returning_and_find_and_update_read_the_refreshed_value_back() -> anyhow::Result<()> {
    let client = seeded(&["a", "b"]).await?;
    let before = Utc::now();

    let returned = update::<Article>()
        .where_(|article| article.id.eq("a"))
        .update(|article| article.title.set("returned"))
        .returning::<Article>()
        .execute(&client)
        .await?;
    assert_eq!(returned.len(), 1);
    assert_refreshed(&returned[0], before);

    let found = find_and_update::<Article>()
        .where_(|article| article.id.eq("b"))
        .update(|article| article.views.increment(5))
        .execute(&client)
        .await?;
    assert_eq!(found.views, 5);
    assert_refreshed(&found, before);
    assert_eq!(article(&client, "b").await?.updated_at, found.updated_at);

    Ok(())
}

#[tokio::test]
async fn an_explicit_value_wins_over_the_database_clock() -> anyhow::Result<()> {
    let client = seeded(&["a"]).await?;
    let chosen = Utc.with_ymd_and_hms(2010, 1, 1, 0, 0, 0).single().expect("valid timestamp");

    update::<Article>()
        .where_(|article| article.id.eq("a"))
        .update(|article| article.title.set("manual"))
        .update(|article| article.updated_at.set(chosen))
        .execute(&client)
        .await?;

    let changed = article(&client, "a").await?;
    assert_eq!(changed.updated_at, chosen);
    // Only the field that was set explicitly keeps its value.
    assert_eq!(changed.touched_on, Utc::now().date_naive());
    assert!(changed.edited_at.is_some_and(|edited_at| edited_at > past()));

    Ok(())
}

#[tokio::test]
async fn updates_inside_a_transaction_refresh_updated_at() -> anyhow::Result<()> {
    let client = seeded(&["a"]).await?;
    let before = Utc::now();

    transaction(&client, |tx| async move {
        update::<Article>()
            .where_(|article| article.id.eq("a"))
            .update(|article| article.title.set("tx"))
            .execute(tx)
            .await?;
        Ok::<_, TransactionError>(())
    })
    .await?;

    assert_refreshed(&article(&client, "a").await?, before);

    Ok(())
}

#[tokio::test]
async fn relation_only_updates_leave_updated_at_alone() -> anyhow::Result<()> {
    let client = seeded(&["a"]).await?;
    dinoco::insert_into::<Tag>().values(Tag::new("rust".to_string(), "Rust".to_string())).execute(&client).await?;

    update::<Article>()
        .where_(|article| article.id.eq("a"))
        .update(|article| article.tag_id.connect("rust"))
        .execute(&client)
        .await?;

    let linked = find_many::<Article>().where_(|article| article.tag_id.eq("rust")).execute(&client).await?;
    assert_eq!(linked.len(), 1);
    assert_untouched(&article(&client, "a").await?);

    Ok(())
}
