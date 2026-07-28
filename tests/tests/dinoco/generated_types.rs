use dinoco::{DinocoEnum, Entity, Snowflake, chrono, serde_json};

#[derive(Debug, Clone, PartialEq, Eq, Default, DinocoEnum)]
enum GeneratedStatus {
    #[default]
    #[dinoco(value = "waiting")]
    Waiting,
    #[dinoco(value = "in-progress")]
    InProgress,
}

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

#[derive(Debug, Entity)]
#[dinoco(table_name = "snow_account")]
struct SnowAccount {
    #[dinoco(primary_key, auto_generate = snowflake)]
    id: Snowflake,

    #[dinoco(many_to_many)]
    systems: Vec<SnowSystem>,
}

#[derive(Debug, Entity)]
#[dinoco(table_name = "snow_system")]
struct SnowSystem {
    #[dinoco(primary_key, auto_generate = snowflake)]
    id: Snowflake,

    #[dinoco(many_to_many)]
    accounts: Vec<SnowAccount>,
}

#[derive(Debug, Entity)]
#[dinoco(table_name = "_snow_account_to_snow_system")]
struct SnowAccountSystem {
    #[dinoco(primary_key)]
    account_id: Snowflake,

    #[dinoco(primary_key)]
    system_id: Snowflake,
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

    let account = SnowAccount::new();
    assert_ne!(account.id, 0);
    assert!(account.systems.is_empty());

    let link = SnowAccountSystem::new(account.id, SnowSystem::new().id);
    let _: Snowflake = link.account_id;
    let _: Snowflake = link.system_id;

    let value = dinoco::DinocoValue::from(&GeneratedStatus::InProgress);
    assert_eq!(value, dinoco::DinocoValue::Enum("GeneratedStatus".to_string(), "in-progress".to_string()));
}
