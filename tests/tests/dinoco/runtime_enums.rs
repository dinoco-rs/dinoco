//! `#[derive(DinocoRuntimeEnum)]`: an enum kept in a plain string column.
//! Models that hold the enum read variants and write strings; models that keep
//! `String` accept the enum wherever they take a value. Covers inserts,
//! `insert_many`, filters (`where_`, `where_complex`, relation filters),
//! `update`, `update_many`, `find_and_update`, `delete_many`, `count`,
//! `exists`, `pluck`/`group_by`, includes, `NULL`, unknown stored values,
//! transactions and the generated conversions.
//!
//! Every scenario runs on SQLite, PostgreSQL and MySQL (the same local servers
//! `docker_adapters.rs` uses).

use std::future::Future;

use dinoco::{
    DinocoRuntimeEnum, DinocoValue, Entity, IntoUpdateValue, count, delete_many, exists, find_and_update, find_first,
    find_many, insert_into, insert_many, transaction, update, update_many,
};
use dinoco_engine::{
    Backend, DinocoAdapter, DinocoClient, DinocoSqlCompiler, MigrationColumnType, MySqlAdapter, PostgresAdapter,
    SqliteAdapter,
};
use dinoco_tests::{column, create_table, drop_table, nullable, primary};

const POSTGRES_URL: &str = "postgres://postgres:postgres@localhost:5432/postgres";
const MYSQL_URL: &str = "mysql://root:root@localhost:3306/mysql";
static POSTGRES_TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
static MYSQL_TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

const TABLES: [&str; 2] = ["re_role", "re_grant"];

macro_rules! on_every_backend {
    ($($scenario:ident),* $(,)?) => {
        $(
            mod $scenario {
                #[tokio::test]
                async fn sqlite() -> anyhow::Result<()> {
                    super::run_on_sqlite(stringify!($scenario), super::$scenario).await
                }

                #[tokio::test]
                async fn postgres() -> anyhow::Result<()> {
                    super::run_on_postgres(super::$scenario).await
                }

                #[tokio::test]
                async fn mysql() -> anyhow::Result<()> {
                    super::run_on_mysql(super::$scenario).await
                }
            }
        )*
    };
}

on_every_backend!(
    writes_strings_and_reads_variants,
    string_fields_accept_the_enum,
    filters,
    updates,
    find_and_update_and_deletes,
    insert_many_pluck_and_group,
    relations,
    transactions,
    unknown_stored_values,
);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, DinocoRuntimeEnum)]
pub enum Permission {
    #[default]
    #[dinoco(value = "permission:view-users")]
    ViewUsers,
    #[dinoco(value = "permission:edit-users")]
    EditUsers,
    #[dinoco(value = "permission:delete-users")]
    DeleteUsers,
}

#[derive(Debug, Clone, Entity)]
#[dinoco(table_name = "re_role")]
pub struct Role {
    #[dinoco(primary_key)]
    id: String,
    name: String,

    #[dinoco(one_to_many, foreign_key = "role_id", references = "id")]
    grants: Vec<Grant>,
}

/// What `field.set_runtime_enum("crate::Permission")` generates for
/// `permission String @default("permission:view-users")` and `fallback String?`.
#[derive(Debug, Clone, Entity)]
#[dinoco(table_name = "re_grant")]
pub struct Grant {
    #[dinoco(primary_key)]
    id: String,
    role_id: String,
    #[dinoco(
        default = <crate::Permission as ::dinoco::DinocoRuntimeEnum>::from_column("permission:view-users").expect("the @default of a runtime enum field must be a value of `crate::Permission`")
    )]
    #[dinoco(enum)]
    permission: crate::Permission,
    #[dinoco(enum)]
    fallback: Option<crate::Permission>,
    note: String,

    #[dinoco(many_to_one, foreign_key = "role_id", references = "id")]
    role: Option<Role>,
}

/// The same table, read through the schema's plain `String` fields.
#[derive(Debug, Clone, Entity)]
#[dinoco(table_name = "re_grant")]
pub struct RawGrant {
    #[dinoco(primary_key)]
    id: String,
    role_id: String,
    permission: String,
    fallback: Option<String>,
    note: String,
}

fn grant(id: &str, role_id: &str, permission: Permission, fallback: Option<Permission>) -> Grant {
    Grant {
        id: id.to_string(),
        role_id: role_id.to_string(),
        permission,
        fallback,
        note: format!("note {id}"),
        role: None,
    }
}

async fn seed(client: &DinocoClient) -> anyhow::Result<()> {
    for (id, name) in [("admin", "Admin"), ("viewer", "Viewer")] {
        insert_into::<Role>()
            .values(Role { id: id.to_string(), name: name.to_string(), grants: Vec::new() })
            .execute(client)
            .await?;
    }
    insert_many::<Grant>()
        .values(vec![
            grant("g1", "admin", Permission::ViewUsers, None),
            grant("g2", "admin", Permission::EditUsers, Some(Permission::ViewUsers)),
            grant("g3", "admin", Permission::DeleteUsers, Some(Permission::EditUsers)),
            grant("g4", "viewer", Permission::ViewUsers, None),
        ])
        .execute(client)
        .await?;
    Ok(())
}

async fn raw(client: &DinocoClient, id: &str) -> anyhow::Result<RawGrant> {
    let id = id.to_string();
    Ok(find_first::<RawGrant>().where_(|x| x.id.eq(id)).execute(client).await?.expect("raw grant"))
}

async fn typed(client: &DinocoClient, id: &str) -> anyhow::Result<Grant> {
    let id = id.to_string();
    Ok(find_first::<Grant>().where_(|x| x.id.eq(id)).execute(client).await?.expect("grant"))
}

async fn ids<F>(client: &DinocoClient, filter: F) -> anyhow::Result<Vec<String>>
where
    F: FnOnce(GrantWhere) -> dinoco::FindWhere,
{
    let mut ids =
        find_many::<Grant>().where_(filter).execute(client).await?.into_iter().map(|x| x.id).collect::<Vec<_>>();
    ids.sort();
    Ok(ids)
}

async fn writes_strings_and_reads_variants(client: DinocoClient) -> anyhow::Result<()> {
    let stored = raw(&client, "g2").await?;
    assert_eq!(stored.permission, "permission:edit-users");
    assert_eq!(stored.fallback.as_deref(), Some("permission:view-users"));
    assert_eq!(raw(&client, "g1").await?.fallback, None);

    let read = typed(&client, "g2").await?;
    assert_eq!(read.permission, Permission::EditUsers);
    assert_eq!(read.fallback, Some(Permission::ViewUsers));
    assert_eq!(typed(&client, "g1").await?.fallback, None);

    // `Grant::new` fills the schema default.
    let defaulted = Grant::new("g5".to_string(), "viewer".to_string(), "fresh".to_string());
    assert_eq!(defaulted.permission, Permission::ViewUsers);
    insert_into::<Grant>().values(&defaulted).execute(&client).await?;
    assert_eq!(raw(&client, "g5").await?.permission, "permission:view-users");
    Ok(())
}

async fn string_fields_accept_the_enum(client: DinocoClient) -> anyhow::Result<()> {
    insert_into::<RawGrant>()
        .values(RawGrant {
            id: "r1".to_string(),
            role_id: "viewer".to_string(),
            permission: Permission::DeleteUsers.into(),
            fallback: Some(String::from(&Permission::EditUsers)),
            note: "raw".to_string(),
        })
        .execute(&client)
        .await?;
    let read = typed(&client, "r1").await?;
    assert_eq!((read.permission, read.fallback), (Permission::DeleteUsers, Some(Permission::EditUsers)));

    let mut filtered = find_many::<RawGrant>()
        .where_(|x| x.permission.eq(Permission::DeleteUsers))
        .execute(&client)
        .await?
        .into_iter()
        .map(|x| x.id)
        .collect::<Vec<_>>();
    filtered.sort();
    assert_eq!(filtered, ["g3", "r1"]);

    update::<RawGrant>()
        .where_(|x| x.id.eq("r1"))
        .update(|x| vec![x.permission.set(Permission::ViewUsers), x.fallback.set(Permission::DeleteUsers)])
        .execute(&client)
        .await?;
    let read = typed(&client, "r1").await?;
    assert_eq!((read.permission, read.fallback), (Permission::ViewUsers, Some(Permission::DeleteUsers)));

    update::<RawGrant>().where_(|x| x.id.eq("r1")).update(|x| x.fallback.set(None::<String>)).execute(&client).await?;
    assert_eq!(typed(&client, "r1").await?.fallback, None);
    Ok(())
}

async fn filters(client: DinocoClient) -> anyhow::Result<()> {
    assert_eq!(ids(&client, |x| x.permission.eq(Permission::ViewUsers)).await?, ["g1", "g4"]);
    assert_eq!(ids(&client, |x| x.permission.neq(Permission::ViewUsers)).await?, ["g2", "g3"]);
    assert_eq!(
        ids(&client, |x| x.permission.batch([Permission::EditUsers, Permission::DeleteUsers])).await?,
        ["g2", "g3"]
    );
    assert!(ids(&client, |x| x.permission.batch(Vec::<Permission>::new())).await?.is_empty());
    assert_eq!(ids(&client, |x| x.fallback.null()).await?, ["g1", "g4"]);
    assert_eq!(ids(&client, |x| x.fallback.eq(Permission::EditUsers)).await?, ["g3"]);
    // A plain string still filters the enum column.
    assert_eq!(ids(&client, |x| x.permission.eq("permission:edit-users")).await?, ["g2"]);

    let mut complex = find_many::<Grant>()
        .where_complex(|x, m| m.or(x.permission.eq(Permission::DeleteUsers), x.fallback.eq(Permission::ViewUsers)))
        .execute(&client)
        .await?
        .into_iter()
        .map(|x| x.id)
        .collect::<Vec<_>>();
    complex.sort();
    assert_eq!(complex, ["g2", "g3"]);

    assert_eq!(count::<Grant>().where_(|x| x.permission.eq(Permission::ViewUsers)).execute(&client).await?.total, 2);
    assert!(exists::<Grant>().where_(|x| x.fallback.eq(Permission::EditUsers)).execute(&client).await?);
    assert!(!exists::<Grant>().where_(|x| x.fallback.eq(Permission::DeleteUsers)).execute(&client).await?);
    Ok(())
}

async fn updates(client: DinocoClient) -> anyhow::Result<()> {
    update::<Grant>()
        .where_(|x| x.id.eq("g1"))
        .update(|x| vec![x.permission.set(Permission::DeleteUsers), x.fallback.set(Permission::EditUsers)])
        .execute(&client)
        .await?;
    let stored = raw(&client, "g1").await?;
    assert_eq!(stored.permission, "permission:delete-users");
    assert_eq!(stored.fallback.as_deref(), Some("permission:edit-users"));

    update::<Grant>().where_(|x| x.id.eq("g1")).update(|x| x.fallback.set(None::<Permission>)).execute(&client).await?;
    assert_eq!(raw(&client, "g1").await?.fallback, None);

    update::<Grant>()
        .where_(|x| x.id.eq("g1"))
        .update(|x| x.fallback.set(Some(Permission::ViewUsers)))
        .execute(&client)
        .await?;
    assert_eq!(typed(&client, "g1").await?.fallback, Some(Permission::ViewUsers));

    update_many::<Grant>()
        .where_(|x| x.permission.eq(Permission::ViewUsers))
        .update(|x| x.permission.set(&Permission::EditUsers))
        .execute(&client)
        .await?;
    assert_eq!(ids(&client, |x| x.permission.eq(Permission::EditUsers)).await?, ["g2", "g4"]);
    Ok(())
}

async fn find_and_update_and_deletes(client: DinocoClient) -> anyhow::Result<()> {
    // MySQL reloads the row by `id` (it has no `RETURNING`), so the filter names it.
    let updated = find_and_update::<Grant>()
        .where_(|x| x.id.eq("g3"))
        .where_(|x| x.permission.eq(Permission::DeleteUsers))
        .update(|x| x.permission.set(Permission::ViewUsers))
        .execute(&client)
        .await?;
    assert_eq!((updated.id.as_str(), updated.permission), ("g3", Permission::ViewUsers));

    delete_many::<Grant>().where_(|x| x.permission.eq(Permission::ViewUsers)).execute(&client).await?;
    assert_eq!(ids(&client, |x| x.id.not_null()).await?, ["g2"]);
    Ok(())
}

async fn insert_many_pluck_and_group(client: DinocoClient) -> anyhow::Result<()> {
    let mut permissions = find_many::<Grant>().pluck(|x| x.permission).execute(&client).await?;
    permissions.sort();
    assert_eq!(
        permissions,
        [Permission::ViewUsers, Permission::ViewUsers, Permission::EditUsers, Permission::DeleteUsers]
    );

    let mut fallbacks = find_many::<Grant>().pluck(|x| x.fallback).execute(&client).await?;
    fallbacks.sort();
    assert_eq!(fallbacks, [None, None, Some(Permission::ViewUsers), Some(Permission::EditUsers)]);

    let mut groups = find_many::<Grant>().pluck(|x| x.id.count()).group_by(|x| x.permission).execute(&client).await?;
    groups.sort();
    assert_eq!(groups, [(Permission::ViewUsers, 2), (Permission::EditUsers, 1), (Permission::DeleteUsers, 1)]);
    Ok(())
}

async fn relations(client: DinocoClient) -> anyhow::Result<()> {
    let mut roles = find_many::<Role>().includes(|x| x.grants()).execute(&client).await?;
    roles.sort_by(|a, b| a.id.cmp(&b.id));
    let mut admin = roles[0].grants.iter().map(|x| x.permission).collect::<Vec<_>>();
    admin.sort();
    assert_eq!(admin, [Permission::ViewUsers, Permission::EditUsers, Permission::DeleteUsers]);
    assert_eq!(roles[1].grants.iter().map(|x| x.permission).collect::<Vec<_>>(), [Permission::ViewUsers]);

    // Relation filters bind the enum like a top-level filter.
    let mut can_delete = find_many::<Role>()
        .where_(|x| x.grants.where_(|g| g.permission.eq(Permission::DeleteUsers)))
        .execute(&client)
        .await?
        .into_iter()
        .map(|x| x.id)
        .collect::<Vec<_>>();
    can_delete.sort();
    assert_eq!(can_delete, ["admin"]);
    let only_view = find_many::<Role>()
        .where_(|x| x.grants.every(|g| g.permission.eq(Permission::ViewUsers)))
        .execute(&client)
        .await?;
    assert_eq!(only_view.iter().map(|x| x.id.as_str()).collect::<Vec<_>>(), ["viewer"]);

    let with_role = find_first::<Grant>()
        .where_(|x| x.fallback.eq(Permission::EditUsers))
        .includes(|x| x.role())
        .execute(&client)
        .await?
        .expect("grant");
    assert_eq!(with_role.role.map(|role| role.name).as_deref(), Some("Admin"));
    Ok(())
}

async fn transactions(client: DinocoClient) -> anyhow::Result<()> {
    let result = transaction(&client, |tx| async move {
        insert_into::<Grant>().values(grant("t1", "viewer", Permission::DeleteUsers, None)).execute(tx).await?;
        let inside = find_many::<Grant>().where_(|x| x.permission.eq(Permission::DeleteUsers)).execute(tx).await?;
        assert_eq!(inside.len(), 2);
        Err::<(), _>(anyhow::anyhow!("roll back"))
    })
    .await;
    assert!(result.is_err());
    assert_eq!(ids(&client, |x| x.permission.eq(Permission::DeleteUsers)).await?, ["g3"]);

    transaction(&client, |tx| async move {
        update::<Grant>()
            .where_(|x| x.id.eq("g4"))
            .update(|x| x.permission.set(Permission::DeleteUsers))
            .execute(tx)
            .await?;
        Ok(())
    })
    .await?;
    assert_eq!(ids(&client, |x| x.permission.eq(Permission::DeleteUsers)).await?, ["g3", "g4"]);
    Ok(())
}

async fn unknown_stored_values(client: DinocoClient) -> anyhow::Result<()> {
    insert_into::<RawGrant>()
        .values(RawGrant {
            id: "x1".to_string(),
            role_id: "viewer".to_string(),
            permission: "permission:unknown".to_string(),
            fallback: None,
            note: "legacy".to_string(),
        })
        .execute(&client)
        .await?;

    // A string no variant maps to fails the read instead of guessing a variant.
    let error = find_first::<Grant>().where_(|x| x.id.eq("x1")).execute(&client).await.expect_err("unknown value");
    assert!(error.to_string().contains("failed to decode"), "{error}");
    // Rows with known values still read.
    assert_eq!(typed(&client, "g1").await?.permission, Permission::ViewUsers);
    Ok(())
}

#[test]
fn derive_generates_string_conversions() {
    assert_eq!(Permission::EditUsers.as_str(), "permission:edit-users");
    assert_eq!(Permission::from_column("permission:delete-users"), Some(Permission::DeleteUsers));
    assert_eq!(Permission::from_column("permission:unknown"), None);
    assert_eq!(Permission::from_column("DeleteUsers"), None, "only the stored values map to variants");

    assert_eq!(DinocoValue::from(Permission::ViewUsers), DinocoValue::String("permission:view-users".to_string()));
    assert_eq!(DinocoValue::from(&Permission::ViewUsers), DinocoValue::String("permission:view-users".to_string()));
    assert_eq!(String::from(Permission::EditUsers), "permission:edit-users");
    assert_eq!(String::from(&Permission::EditUsers), "permission:edit-users");
    assert_eq!(Permission::DeleteUsers.to_string(), "permission:delete-users");
    assert_eq!("permission:view-users".parse::<Permission>(), Ok(Permission::ViewUsers));
    assert!("permission:unknown".parse::<Permission>().is_err());
    assert_eq!(Permission::try_from("permission:edit-users".to_string()), Ok(Permission::EditUsers));

    let string_value = |value: DinocoValue| assert_eq!(value, DinocoValue::String("permission:edit-users".into()));
    string_value(IntoUpdateValue::<String>::into_update_value(Permission::EditUsers));
    string_value(IntoUpdateValue::<String>::into_update_value(&Permission::EditUsers));
    string_value(IntoUpdateValue::<Option<String>>::into_update_value(Permission::EditUsers));
    string_value(IntoUpdateValue::<Option<Permission>>::into_update_value(&Permission::EditUsers));
}

#[test]
fn generated_types_take_the_enum() {
    let where_ = GrantWhere::default();
    let _: dinoco::Field<Permission> = where_.permission;
    let _: dinoco::Field<Option<Permission>> = where_.fallback;
    let update = GrantUpdate::default();
    let _: dinoco::UpdateField<Permission> = update.permission;
    let _: dinoco::Pluck<Grant, Permission> = find_many::<Grant>().pluck(|x| x.permission);
    let _: dinoco::Pluck<Grant, Option<Permission>> = find_many::<Grant>().pluck(|x| x.fallback);
}

fn monotonic() -> u128 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
}

async fn run_on_sqlite<F, Fut>(name: &str, scenario: F) -> anyhow::Result<()>
where
    F: FnOnce(DinocoClient) -> Fut,
    Fut: Future<Output = anyhow::Result<()>>,
{
    let path = format!("/private/tmp/dinoco-re-{name}-{}-{}.sqlite", std::process::id(), monotonic());
    let adapter = SqliteAdapter::new(path.clone()).await.map_err(anyhow::Error::msg)?;
    reset_schema(&adapter).await?;
    let result = run_seeded(DinocoClient::new(Backend::Sqlite(adapter)), scenario).await;
    let _ = std::fs::remove_file(path);
    result
}

async fn run_on_postgres<F, Fut>(scenario: F) -> anyhow::Result<()>
where
    F: FnOnce(DinocoClient) -> Fut,
    Fut: Future<Output = anyhow::Result<()>>,
{
    let _guard = POSTGRES_TEST_LOCK.lock().await;
    let adapter = PostgresAdapter::direct(POSTGRES_URL).await?;
    reset_schema(&adapter).await?;
    run_seeded(DinocoClient::new(Backend::Postgres(adapter)), scenario).await
}

async fn run_on_mysql<F, Fut>(scenario: F) -> anyhow::Result<()>
where
    F: FnOnce(DinocoClient) -> Fut,
    Fut: Future<Output = anyhow::Result<()>>,
{
    let _guard = MYSQL_TEST_LOCK.lock().await;
    let adapter = MySqlAdapter::new(MYSQL_URL);
    reset_schema(&adapter).await?;
    run_seeded(DinocoClient::new(Backend::Mysql(adapter)), scenario).await
}

async fn run_seeded<F, Fut>(client: DinocoClient, scenario: F) -> anyhow::Result<()>
where
    F: FnOnce(DinocoClient) -> Fut,
    Fut: Future<Output = anyhow::Result<()>>,
{
    seed(&client).await?;
    scenario(client).await
}

async fn reset_schema<A>(adapter: &A) -> anyhow::Result<()>
where
    A: DinocoAdapter + DinocoSqlCompiler,
{
    for table in TABLES.iter().rev() {
        drop_table(adapter, table).await?;
    }

    // The permission columns are ordinary strings: no enum type exists.
    let string = |name: &str| column(name, MigrationColumnType::String);
    create_table(adapter, "re_role", vec![primary(string("id")), string("name")]).await?;
    create_table(
        adapter,
        "re_grant",
        vec![
            primary(string("id")),
            string("role_id"),
            string("permission"),
            nullable(string("fallback")),
            string("note"),
        ],
    )
    .await
}
