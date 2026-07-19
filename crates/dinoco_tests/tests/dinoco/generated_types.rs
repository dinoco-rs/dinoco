use dinoco::{Entity, chrono, serde_json};

#[derive(Debug, Entity)]
#[dinoco(table_name = "generated_scalar_fixture")]
struct GeneratedScalarFixture {
    id: String,
    created_at: chrono::DateTime<chrono::Utc>,
    birthday: chrono::NaiveDate,
    metadata: serde_json::Value,
}

#[derive(Debug, Entity)]
#[dinoco(table_name = "count_parent_fixture")]
struct CountParentFixture {
    id: String,

    #[dinoco(one_to_many, foreign_key = "parent_id", references = "id")]
    first_children: Vec<FirstChildFixture>,

    #[dinoco(one_to_many, foreign_key = "parent_id", references = "id")]
    second_children: Vec<SecondChildFixture>,
}

#[derive(Debug, Entity)]
#[dinoco(table_name = "first_child_fixture")]
struct FirstChildFixture {
    id: String,
    parent_id: String,

    #[dinoco(many_to_one, foreign_key = "parent_id", references = "id")]
    parent: Option<CountParentFixture>,
}

#[derive(Debug, Entity)]
#[dinoco(table_name = "second_child_fixture")]
struct SecondChildFixture {
    id: String,
    parent_id: String,

    #[dinoco(many_to_one, foreign_key = "parent_id", references = "id")]
    parent: Option<CountParentFixture>,
}

#[test]
fn generated_scalar_types_satisfy_entity_adapter_bounds() {
    let _ = GeneratedScalarFixture::new(
        "fixture".to_string(),
        chrono::Utc::now(),
        chrono::Utc::now().date_naive(),
        serde_json::json!({ "source": "codegen" }),
    );

    let selector = CountParentFixtureCountInclude::default();
    let _ = selector.first_children();
    let _ = selector.second_children();

    let result = CountParentFixtureCount::default();
    let _: Option<i64> = result.first_children;
    let _: Option<i64> = result.second_children;
}
