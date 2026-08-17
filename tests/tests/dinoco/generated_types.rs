#![allow(clippy::needless_borrows_for_generic_args)]

use dinoco::{DinocoEnum, Entity, Snowflake, chrono, serde_json};

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, dinoco::serde::Serialize, dinoco::serde::Deserialize, DinocoEnum,
)]
#[serde(crate = "::dinoco::serde")]
enum GeneratedStatus {
    #[default]
    #[dinoco(value = "waiting")]
    Waiting,
    #[dinoco(value = "in-progress")]
    InProgress,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, dinoco::serde::Serialize, dinoco::serde::Deserialize, DinocoEnum,
)]
#[serde(crate = "::dinoco::serde")]
enum AudioStatus {
    #[default]
    #[dinoco(value = "generated")]
    Generated,
    #[dinoco(value = "building")]
    Building,
    #[dinoco(value = "error")]
    Error,
}

#[derive(Debug, Entity)]
#[dinoco(table_name = "enum_update_fixture")]
struct EnumUpdateFixture {
    id: String,
    status: GeneratedStatus,
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
    assert_eq!(GeneratedStatus::Waiting.to_string(), "waiting");
    assert_eq!(GeneratedStatus::InProgress.to_string(), "in-progress");
    assert_eq!(GeneratedStatus::try_from("waiting"), Ok(GeneratedStatus::Waiting));
    assert_eq!(GeneratedStatus::try_from("in-progress".to_string()), Ok(GeneratedStatus::InProgress));
    assert_eq!("in-progress".parse::<GeneratedStatus>(), Ok(GeneratedStatus::InProgress));
    assert_eq!(
        GeneratedStatus::try_from("InProgress"),
        Err("unknown value `InProgress` for enum `GeneratedStatus`".to_string()),
    );

    let json = serde_json::to_string(&GeneratedStatus::InProgress).expect("serialize generated enum");
    assert_eq!(json, "\"InProgress\"");
    assert_eq!(
        serde_json::from_str::<GeneratedStatus>(&json).expect("deserialize generated enum"),
        GeneratedStatus::InProgress
    );

    let postgres_enum = dinoco::tokio_postgres::types::Type::new(
        "GeneratedStatus".to_string(),
        99_999,
        dinoco::tokio_postgres::types::Kind::Enum(vec!["waiting".to_string(), "in-progress".to_string()]),
        "public".to_string(),
    );
    assert!(<GeneratedStatus as dinoco::tokio_postgres::types::FromSql>::accepts(&postgres_enum));
    assert_eq!(
        <GeneratedStatus as dinoco::tokio_postgres::types::FromSql>::from_sql(&postgres_enum, b"in-progress")
            .expect("decode native PostgreSQL enum"),
        GeneratedStatus::InProgress
    );
}

#[test]
fn generated_enums_can_be_used_by_all_update_builders() {
    let _ = dinoco::update::<EnumUpdateFixture>().update(|item| item.status.set(GeneratedStatus::InProgress));
    let _ = dinoco::find_and_update::<EnumUpdateFixture>().update(|item| item.status.set(&GeneratedStatus::Waiting));
    let _ = dinoco::update_many::<EnumUpdateFixture>().update(|item| item.status.set(GeneratedStatus::Waiting));
    let _ =
        dinoco::UpdateField::<Option<GeneratedStatus>>::new("optional_status").set(Some(GeneratedStatus::InProgress));
}

#[test]
fn generated_enums_compile_in_every_query_builder() {
    let waiting = GeneratedStatus::Waiting;
    let _ = dinoco::find_first::<EnumUpdateFixture>().where_(|item| item.status.eq(GeneratedStatus::Waiting));
    let _ = dinoco::find_many::<EnumUpdateFixture>().where_(|item| item.status.neq(&waiting));
    let _ = dinoco::count::<EnumUpdateFixture>().where_(|item| item.status.batch([GeneratedStatus::Waiting]));
    let _ = dinoco::find_and_update::<EnumUpdateFixture>()
        .where_(|item| item.status.eq(&waiting))
        .update(|item| item.status.set(GeneratedStatus::InProgress));
    let _ = dinoco::update::<EnumUpdateFixture>()
        .where_(|item| item.status.neq(GeneratedStatus::InProgress))
        .update(|item| item.status.set(&waiting));
    let _ = dinoco::update_many::<EnumUpdateFixture>()
        .where_(|item| item.status.batch([&waiting]))
        .update(|item| item.status.set(GeneratedStatus::InProgress));
    let _ = dinoco::delete::<EnumUpdateFixture>().where_(|item| item.status.eq(GeneratedStatus::Waiting));
    let _ = dinoco::delete_many::<EnumUpdateFixture>().where_(|item| item.status.neq(&waiting));
}

#[test]
fn generated_enum_error_variant_does_not_conflict_with_associated_error_types() {
    assert_eq!(AudioStatus::try_from("error"), Ok(AudioStatus::Error));
    assert_eq!("building".parse::<AudioStatus>(), Ok(AudioStatus::Building));
    assert_eq!(AudioStatus::Error.to_string(), "error");
    assert_eq!(
        dinoco::DinocoValue::from(&AudioStatus::Error),
        dinoco::DinocoValue::Enum("AudioStatus".to_string(), "error".to_string())
    );
    let _ = dinoco::UpdateField::<AudioStatus>::new("status").set(AudioStatus::Error);
    let _ = dinoco::UpdateField::<AudioStatus>::new("status").set(&AudioStatus::Building);
    let _ = dinoco::UpdateField::<Option<AudioStatus>>::new("status").set(Some(AudioStatus::Generated));
}

#[test]
fn generated_enums_can_be_used_by_value_in_filters() {
    let owned = dinoco::DinocoValue::from(GeneratedStatus::InProgress);
    assert_eq!(owned, dinoco::DinocoValue::Enum("GeneratedStatus".to_string(), "in-progress".to_string()));

    let status = dinoco::Field::<GeneratedStatus>::new("status");
    let _ = status.eq(GeneratedStatus::Waiting);
    let _ = status.neq(GeneratedStatus::InProgress);
    let _ = status.batch([GeneratedStatus::Waiting, GeneratedStatus::InProgress]);

    let error_fields = dinoco::Field::<AudioStatus>::new("status");
    let _ = error_fields.eq(AudioStatus::Error);
    let _ = error_fields.neq(AudioStatus::Building);

    let waiting = GeneratedStatus::Waiting;
    let in_progress = GeneratedStatus::InProgress;
    let status = dinoco::Field::<GeneratedStatus>::new("status");
    let _ = status.eq(&waiting);
    let _ = status.neq(&in_progress);
    let _ = status.gt(GeneratedStatus::Waiting);
    let _ = status.gt(&waiting);
    let _ = status.gte(GeneratedStatus::Waiting);
    let _ = status.gte(&waiting);
    let _ = status.lt(GeneratedStatus::InProgress);
    let _ = status.lt(&in_progress);
    let _ = status.lte(GeneratedStatus::InProgress);
    let _ = status.lte(&in_progress);
    let _ = status.batch([&waiting, &in_progress]);
    let _ = status.null();
    let _ = status.not_null();
}

#[test]
fn every_generated_scalar_type_can_be_used_by_update_builders() {
    let now = chrono::Utc::now();
    let date = now.date_naive();
    let metadata = serde_json::json!({ "updated": true });

    let _ = dinoco::update::<GeneratedScalarFixture>().update(|item| item.created_at.set(now));
    let _ = dinoco::update::<GeneratedScalarFixture>().update(|item| item.created_at.set(&now));
    let _ = dinoco::find_and_update::<GeneratedScalarFixture>().update(|item| item.birthday.set(date));
    let _ = dinoco::find_and_update::<GeneratedScalarFixture>().update(|item| item.birthday.set(&date));
    let _ = dinoco::update_many::<GeneratedScalarFixture>().update(|item| item.metadata.set(metadata.clone()));
    let _ = dinoco::update_many::<GeneratedScalarFixture>().update(|item| item.metadata.set(&metadata));

    let _ = dinoco::UpdateField::<Option<chrono::DateTime<chrono::Utc>>>::new("optional_datetime").set(Some(now));
    let _ = dinoco::UpdateField::<Option<chrono::NaiveDate>>::new("optional_date").set(Some(date));
    let _ = dinoco::UpdateField::<Option<serde_json::Value>>::new("optional_json").set(Some(metadata));
    let _ = dinoco::UpdateField::<Option<serde_json::Value>>::new("optional_json").set(None);

    let optional_now = Some(now);
    let optional_date = Some(date);
    let optional_metadata = Some(serde_json::json!({ "borrowed": true }));
    let optional_status = Some(GeneratedStatus::Waiting);
    let no_metadata: Option<serde_json::Value> = None;
    let _ = dinoco::UpdateField::<Option<chrono::DateTime<chrono::Utc>>>::new("optional_datetime").set(&optional_now);
    let _ = dinoco::UpdateField::<Option<chrono::NaiveDate>>::new("optional_date").set(&optional_date);
    let _ = dinoco::UpdateField::<Option<serde_json::Value>>::new("optional_json").set(&optional_metadata);
    let _ = dinoco::UpdateField::<Option<GeneratedStatus>>::new("optional_status").set(&optional_status);
    let _ = dinoco::UpdateField::<Option<serde_json::Value>>::new("optional_json").set(&no_metadata);
}

#[test]
fn datetime_date_and_json_support_every_common_filter_operator() {
    let first_datetime = chrono::Utc::now();
    let second_datetime = first_datetime + chrono::Duration::hours(1);
    let datetime = dinoco::Field::<chrono::DateTime<chrono::Utc>>::new("created_at");
    let _ = datetime.eq(first_datetime);
    let _ = datetime.eq(&first_datetime);
    let _ = datetime.neq(second_datetime);
    let _ = datetime.neq(&second_datetime);
    let _ = datetime.gt(first_datetime);
    let _ = datetime.gt(&first_datetime);
    let _ = datetime.gte(first_datetime);
    let _ = datetime.gte(&first_datetime);
    let _ = datetime.lt(second_datetime);
    let _ = datetime.lt(&second_datetime);
    let _ = datetime.lte(second_datetime);
    let _ = datetime.lte(&second_datetime);
    let _ = datetime.batch([first_datetime, second_datetime]);
    let _ = datetime.batch([&first_datetime, &second_datetime]);
    let _ = datetime.between(first_datetime, second_datetime);
    let _ = datetime.between(&first_datetime, &second_datetime);
    let _ = datetime.null();
    let _ = datetime.not_null();

    let first_date = first_datetime.date_naive();
    let second_date = second_datetime.date_naive();
    let date = dinoco::Field::<chrono::NaiveDate>::new("birthday");
    let _ = date.eq(first_date);
    let _ = date.eq(&first_date);
    let _ = date.neq(second_date);
    let _ = date.neq(&second_date);
    let _ = date.gt(first_date);
    let _ = date.gt(&first_date);
    let _ = date.gte(first_date);
    let _ = date.gte(&first_date);
    let _ = date.lt(second_date);
    let _ = date.lt(&second_date);
    let _ = date.lte(second_date);
    let _ = date.lte(&second_date);
    let _ = date.batch([first_date, second_date]);
    let _ = date.batch([&first_date, &second_date]);
    let _ = date.between(first_date, second_date);
    let _ = date.between(&first_date, &second_date);
    let _ = date.null();
    let _ = date.not_null();

    let first_json = serde_json::json!({ "order": 1 });
    let second_json = serde_json::json!({ "order": 2 });
    let json = dinoco::Field::<serde_json::Value>::new("metadata");
    let _ = json.eq(first_json.clone());
    let _ = json.eq(&first_json);
    let _ = json.neq(second_json.clone());
    let _ = json.neq(&second_json);
    let _ = json.gt(first_json.clone());
    let _ = json.gt(&first_json);
    let _ = json.gte(first_json.clone());
    let _ = json.gte(&first_json);
    let _ = json.lt(second_json.clone());
    let _ = json.lt(&second_json);
    let _ = json.lte(second_json.clone());
    let _ = json.lte(&second_json);
    let _ = json.batch([first_json.clone(), second_json.clone()]);
    let _ = json.batch([&first_json, &second_json]);
    let _ = json.null();
    let _ = json.not_null();
}

#[test]
fn generated_scalar_values_compile_in_every_query_builder() {
    let now = chrono::Utc::now();
    let date = now.date_naive();
    let metadata = serde_json::json!({ "query": true });

    let _ = dinoco::find_first::<GeneratedScalarFixture>()
        .where_(|item| item.created_at.eq(now))
        .where_(|item| item.birthday.eq(date))
        .where_(|item| item.metadata.eq(metadata.clone()));
    let _ = dinoco::find_many::<GeneratedScalarFixture>()
        .where_(|item| item.created_at.neq(&now))
        .where_(|item| item.birthday.gte(&date))
        .where_(|item| item.metadata.neq(&metadata));
    let _ = dinoco::count::<GeneratedScalarFixture>()
        .where_(|item| item.created_at.lte(now))
        .where_(|item| item.birthday.lte(date))
        .where_(|item| item.metadata.eq(metadata.clone()));
    let _ = dinoco::find_and_update::<GeneratedScalarFixture>()
        .where_(|item| item.created_at.eq(&now))
        .where_(|item| item.birthday.eq(&date))
        .where_(|item| item.metadata.eq(&metadata))
        .update(|item| item.created_at.set(now))
        .update(|item| item.birthday.set(date))
        .update(|item| item.metadata.set(metadata.clone()));
    let _ = dinoco::update::<GeneratedScalarFixture>()
        .where_(|item| item.created_at.eq(now))
        .where_(|item| item.birthday.eq(date))
        .where_(|item| item.metadata.eq(metadata.clone()))
        .update(|item| item.created_at.set(&now))
        .update(|item| item.birthday.set(&date))
        .update(|item| item.metadata.set(&metadata));
    let _ = dinoco::update_many::<GeneratedScalarFixture>()
        .where_(|item| item.created_at.batch([now]))
        .where_(|item| item.birthday.batch([date]))
        .where_(|item| item.metadata.batch([metadata.clone()]))
        .update(|item| item.created_at.set(now))
        .update(|item| item.birthday.set(date))
        .update(|item| item.metadata.set(metadata.clone()));
    let _ = dinoco::delete::<GeneratedScalarFixture>()
        .where_(|item| item.created_at.eq(&now))
        .where_(|item| item.birthday.eq(&date))
        .where_(|item| item.metadata.eq(&metadata));
    let _ = dinoco::delete_many::<GeneratedScalarFixture>()
        .where_(|item| item.created_at.eq(now))
        .where_(|item| item.birthday.eq(date))
        .where_(|item| item.metadata.eq(metadata));
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
