use dinoco::{DinocoEnum, Entity, Snowflake, chrono, serde_json};

#[derive(Debug, Clone, PartialEq, Eq, Default, dinoco::serde::Serialize, dinoco::serde::Deserialize, DinocoEnum)]
#[serde(crate = "::dinoco::serde")]
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
#[dinoco(table_name = "multiple_relation_business")]
struct MultipleRelationBusiness {
    id: String,

    #[dinoco(one_to_many, relation_name = "owned", foreign_key = "owned_business_id", references = "id")]
    analyses: Vec<MultipleRelationAnalyse>,

    #[dinoco(one_to_many, relation_name = "changes", foreign_key = "changes_business_id", references = "id")]
    registration_changes: Vec<MultipleRelationAnalyse>,
}

#[derive(Debug, Entity)]
#[dinoco(table_name = "multiple_relation_analyse")]
struct MultipleRelationAnalyse {
    id: String,
    owned_business_id: Option<String>,

    #[dinoco(many_to_one, relation_name = "owned", foreign_key = "owned_business_id", references = "id")]
    owned_business: Option<MultipleRelationBusiness>,

    changes_business_id: Option<String>,

    #[dinoco(many_to_one, relation_name = "changes", foreign_key = "changes_business_id", references = "id")]
    changes_business: Option<MultipleRelationBusiness>,
}

#[derive(Debug, Default, dinoco::EntityExtend)]
#[extend(MultipleRelationBusiness)]
struct MultipleRelationBusinessProjection {
    id: String,
    analyses: Vec<MultipleRelationAnalyse>,
    registration_changes: Vec<MultipleRelationAnalyse>,
}

#[derive(Debug, Entity)]
#[dinoco(table_name = "snow_account")]
struct SnowAccount {
    #[dinoco(primary_key, auto_generate = snowflake)]
    id: Snowflake,

    #[dinoco(
        many_to_many,
        foreign_key = "id",
        references = "id",
        join_table = "_snow_account_to_snow_system",
        parent_field = "id",
        join_parent_field = "account_id",
        join_child_field = "system_id"
    )]
    systems: Vec<SnowSystem>,

    #[dinoco(
        many_to_many_key,
        join_table = "_snow_account_to_snow_system",
        parent_field = "id",
        join_parent_field = "account_id",
        join_child_field = "system_id"
    )]
    system_id: Option<Snowflake>,
}

#[derive(Debug, Entity)]
#[dinoco(table_name = "snow_system")]
struct SnowSystem {
    #[dinoco(primary_key, auto_generate = snowflake)]
    id: Snowflake,

    #[dinoco(
        many_to_many,
        foreign_key = "id",
        references = "id",
        join_table = "_snow_account_to_snow_system",
        parent_field = "id",
        join_parent_field = "system_id",
        join_child_field = "account_id"
    )]
    accounts: Vec<SnowAccount>,

    #[dinoco(
        many_to_many_key,
        join_table = "_snow_account_to_snow_system",
        parent_field = "id",
        join_parent_field = "system_id",
        join_child_field = "account_id"
    )]
    account_id: Option<Snowflake>,
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
    assert!(account.system_id.is_none());
    assert!(SnowSystem::new().account_id.is_none());

    let value = dinoco::DinocoValue::from(&GeneratedStatus::InProgress);
    assert_eq!(value, dinoco::DinocoValue::Enum("GeneratedStatus".to_string(), "in-progress".to_string()));

    let json = serde_json::to_string(&GeneratedStatus::InProgress).expect("serialize generated enum");
    assert_eq!(json, "\"InProgress\"");
    assert_eq!(
        serde_json::from_str::<GeneratedStatus>(&json).expect("deserialize generated enum"),
        GeneratedStatus::InProgress
    );
}

#[test]
fn derive_dispatches_multiple_relations_to_the_same_model_independently() {
    let mut analyse = MultipleRelationAnalyse::default();
    let owned = MultipleRelationBusiness { id: "owned".to_string(), ..Default::default() };
    let changes = MultipleRelationBusiness { id: "changes".to_string(), ..Default::default() };

    <MultipleRelationAnalyse as dinoco::DinocoRelationApply<MultipleRelationBusiness>>::dinoco_apply_one(
        &mut analyse,
        "owned_business",
        Some(owned),
    );
    <MultipleRelationAnalyse as dinoco::DinocoRelationApply<MultipleRelationBusiness>>::dinoco_apply_one(
        &mut analyse,
        "changes_business",
        Some(changes),
    );

    assert_eq!(analyse.owned_business.as_ref().map(|business| business.id.as_str()), Some("owned"));
    assert_eq!(analyse.changes_business.as_ref().map(|business| business.id.as_str()), Some("changes"));

    let parent = MultipleRelationBusiness { id: "parent".to_string(), ..Default::default() };
    <MultipleRelationAnalyse as dinoco::DinocoBelongsTo<MultipleRelationBusiness>>::dinoco_bind_parent_relation(
        &mut analyse,
        "changes",
        &parent,
    );

    assert_eq!(analyse.owned_business_id, None);
    assert_eq!(analyse.changes_business_id.as_deref(), Some("parent"));

    let mut business = MultipleRelationBusiness::default();
    <MultipleRelationBusiness as dinoco::DinocoRelationApply<MultipleRelationAnalyse>>::dinoco_apply_many(
        &mut business,
        "analyses",
        vec![MultipleRelationAnalyse::default()],
    );
    <MultipleRelationBusiness as dinoco::DinocoRelationApply<MultipleRelationAnalyse>>::dinoco_apply_many(
        &mut business,
        "registration_changes",
        vec![MultipleRelationAnalyse::default(), MultipleRelationAnalyse::default()],
    );

    assert_eq!(business.analyses.len(), 1);
    assert_eq!(business.registration_changes.len(), 2);
}

#[test]
fn entity_extend_dispatches_multiple_relations_to_the_same_model_independently() {
    let mut projection = MultipleRelationBusinessProjection::default();

    <MultipleRelationBusinessProjection as dinoco::DinocoRelationApply<MultipleRelationAnalyse>>::dinoco_apply_many(
        &mut projection,
        "analyses",
        vec![MultipleRelationAnalyse::default()],
    );
    <MultipleRelationBusinessProjection as dinoco::DinocoRelationApply<MultipleRelationAnalyse>>::dinoco_apply_many(
        &mut projection,
        "registration_changes",
        vec![MultipleRelationAnalyse::default(), MultipleRelationAnalyse::default()],
    );

    assert_eq!(projection.analyses.len(), 1);
    assert_eq!(projection.registration_changes.len(), 2);
}
