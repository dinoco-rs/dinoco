use dinoco::{Entity, EntityExtend, find_first, find_many, insert_into, insert_many};
use dinoco_engine::{
    Backend, CreateTableMigration, DinocoAdapter, DinocoClient, DinocoSqlCompiler, MigrationColumn,
    MigrationColumnType, MigrationDefault, MigrationForeignKey, MySqlAdapter, PostgresAdapter, ReferentialAction,
    SqliteAdapter,
};
use dinoco_tests::{column, create_table, default, drop_table, nullable, primary};

const POSTGRES_URL: &str = "postgres://postgres:postgres@localhost:5432/postgres";
const MYSQL_URL: &str = "mysql://root:root@localhost:3306/mysql";

#[derive(Debug, Entity)]
#[dinoco(table_name = "all_methods_user")]
pub struct User {
    #[dinoco(auto_generate = uuid)]
    id: String,
    email: String,
    office: String,
    age: i64,

    #[dinoco(one_to_many, foreign_key = "user_id", references = "id")]
    tokens: Vec<Token>,
}

#[derive(Debug, Entity)]
#[dinoco(table_name = "all_methods_token")]
pub struct Token {
    #[dinoco(auto_generate = uuid)]
    id: String,
    #[dinoco(default = false)]
    is_expired: bool,
    user_id: Option<String>,

    #[dinoco(one_to_many, foreign_key = "user_id", references = "id")]
    user: Option<User>,
}

#[derive(Debug, Entity)]
#[dinoco(table_name = "all_methods_user_post")]
pub struct UserPost {
    user_id: String,
    post_id: String,
}

#[derive(Debug, EntityExtend)]
#[extend(User)]
pub struct UserSelect {
    id: String,
    email: String,
}

#[tokio::test]
async fn sqlite_adapter_runs_all_dinoco_methods() -> anyhow::Result<()> {
    let path = format!("/private/tmp/dinoco-all-methods-{}-{}.sqlite", std::process::id(), monotonic());
    let adapter = SqliteAdapter::new(path.clone()).await.map_err(anyhow::Error::msg)?;
    reset_schema(&adapter).await?;
    run_all_methods(DinocoClient::new(Backend::Sqlite(adapter))).await?;
    let _ = std::fs::remove_file(path);
    Ok(())
}

#[tokio::test]
async fn postgres_adapter_runs_all_dinoco_methods() -> anyhow::Result<()> {
    let adapter = PostgresAdapter::direct(POSTGRES_URL).await?;
    reset_schema(&adapter).await?;
    run_all_methods(DinocoClient::new(Backend::Postgres(adapter))).await
}

#[tokio::test]
async fn mysql_adapter_runs_all_dinoco_methods() -> anyhow::Result<()> {
    let adapter = MySqlAdapter::new(MYSQL_URL);
    reset_schema(&adapter).await?;
    run_all_methods(DinocoClient::new(Backend::Mysql(adapter))).await
}

#[tokio::test]
async fn sqlite_adapter_detects_migration_changes_from_introspection() -> anyhow::Result<()> {
    let path = format!("/private/tmp/dinoco-detect-{}-{}.sqlite", std::process::id(), monotonic());
    let adapter = SqliteAdapter::new(path.clone()).await.map_err(anyhow::Error::msg)?;
    reset_detection_schema(&adapter).await?;
    seed_detection_schema(&adapter).await?;
    assert_detection_plan(dinoco_cli::db::CliDatabase::Sqlite(adapter)).await?;
    let _ = std::fs::remove_file(path);
    Ok(())
}

#[tokio::test]
async fn postgres_adapter_detects_migration_changes_from_introspection() -> anyhow::Result<()> {
    let adapter = PostgresAdapter::direct(POSTGRES_URL).await?;
    reset_detection_schema(&adapter).await?;
    seed_detection_schema(&adapter).await?;
    assert_detection_plan(dinoco_cli::db::CliDatabase::Postgres(adapter)).await
}

#[tokio::test]
async fn mysql_adapter_detects_migration_changes_from_introspection() -> anyhow::Result<()> {
    let adapter = MySqlAdapter::new(MYSQL_URL);
    reset_detection_schema(&adapter).await?;
    seed_detection_schema(&adapter).await?;
    assert_detection_plan(dinoco_cli::db::CliDatabase::Mysql(adapter)).await
}

async fn reset_schema<A>(adapter: &A) -> anyhow::Result<()>
where
    A: DinocoAdapter + DinocoSqlCompiler,
{
    drop_table(adapter, "all_methods_user_post").await?;
    drop_table(adapter, "all_methods_token").await?;
    drop_table(adapter, "all_methods_user").await?;

    create_table(
        adapter,
        "all_methods_user",
        vec![
            primary(column("id", MigrationColumnType::String)),
            column("email", MigrationColumnType::String),
            column("office", MigrationColumnType::String),
            column("age", MigrationColumnType::Integer),
        ],
    )
    .await?;
    create_table(
        adapter,
        "all_methods_token",
        vec![
            primary(column("id", MigrationColumnType::String)),
            default(column("is_expired", MigrationColumnType::Boolean), MigrationDefault::Boolean(false)),
            nullable(column("user_id", MigrationColumnType::String)),
        ],
    )
    .await?;
    create_table(
        adapter,
        "all_methods_user_post",
        vec![column("user_id", MigrationColumnType::String), column("post_id", MigrationColumnType::String)],
    )
    .await?;

    Ok(())
}

async fn reset_detection_schema<A>(adapter: &A) -> anyhow::Result<()>
where
    A: DinocoAdapter + DinocoSqlCompiler,
{
    drop_table(adapter, "migration_detect_old_rel").await?;
    drop_table(adapter, "migration_detect_post").await?;
    drop_table(adapter, "migration_detect_user").await?;
    Ok(())
}

async fn seed_detection_schema<A>(adapter: &A) -> anyhow::Result<()>
where
    A: DinocoAdapter + DinocoSqlCompiler,
{
    let user_id = MigrationColumn {
        name: "id".to_string(),
        ty: MigrationColumnType::Integer,
        primary_key: true,
        nullable: false,
        default: None,
    };
    adapter
        .execute(
            &adapter.compile_create_table_migration(CreateTableMigration {
                table: "migration_detect_user".to_string(),
                columns: vec![
                    user_id.clone(),
                    column("name", MigrationColumnType::String),
                    column("legacy", MigrationColumnType::Integer),
                ],
                foreign_keys: Vec::new(),
                if_not_exists: false,
            }),
            &[],
        )
        .await?;
    adapter
        .execute(
            &adapter.compile_create_table_migration(CreateTableMigration {
                table: "migration_detect_post".to_string(),
                columns: vec![user_id.clone(), nullable(column("user_id", MigrationColumnType::Integer))],
                foreign_keys: Vec::new(),
                if_not_exists: false,
            }),
            &[],
        )
        .await?;
    adapter
        .execute(
            &adapter.compile_create_table_migration(CreateTableMigration {
                table: "migration_detect_old_rel".to_string(),
                columns: vec![user_id, column("user_id", MigrationColumnType::Integer)],
                foreign_keys: vec![MigrationForeignKey {
                    name: "fk_migration_detect_old_rel_user_id".to_string(),
                    columns: vec!["user_id".to_string()],
                    references_table: "migration_detect_user".to_string(),
                    references_columns: vec!["id".to_string()],
                    on_update: ReferentialAction::NoAction,
                    on_delete: ReferentialAction::NoAction,
                }],
                if_not_exists: false,
            }),
            &[],
        )
        .await?;

    adapter.execute("INSERT INTO migration_detect_user (id, name, legacy) VALUES (1, 'old', 10)", &[]).await?;
    Ok(())
}

async fn assert_detection_plan(db: dinoco_cli::db::CliDatabase) -> anyhow::Result<()> {
    let mut current = db.inspect_schema().await?;
    current.tables.retain(|table| table.name.starts_with("migration_detect_"));
    current.enums.clear();

    let desired = dinoco_compiler::compile(
        r#"
        config {
            database = "postgresql"
            database_url = env("DATABASE_URL")
        }

        model MigrationDetectUser {
            id        Integer @id
            full_name String
        }

        model MigrationDetectPost {
            id      Integer @id
            user_id Integer?
            user    MigrationDetectUser? @relation(fields: [user_id], references: [id], onDelete: SetNull, onUpdate: Cascade)
        }

        model MigrationDetectOldRel {
            id      Integer @id
            user_id Integer
        }
        "#,
    )?;
    let plan = dinoco_cli::sql::plan_schema_migration(&desired, &current);

    assert!(
        plan.steps.iter().any(
            |step| matches!(step, dinoco_cli::sql::MigrationStep::RenameColumn(item) if item.table == "migration_detect_user" && item.from == "name" && item.to == "full_name")
        ),
        "plan did not detect rename: {plan:#?}"
    );
    assert!(
        plan.steps.iter().any(
            |step| matches!(step, dinoco_cli::sql::MigrationStep::DropColumn(item) if item.table == "migration_detect_user" && item.column == "legacy")
        ),
        "plan did not detect dropped column: {plan:#?}"
    );
    assert!(
        plan.steps.iter().any(
            |step| matches!(step, dinoco_cli::sql::MigrationStep::AddForeignKey(item) if item.table == "migration_detect_post" && item.foreign_key.on_delete == ReferentialAction::SetNull)
        ),
        "plan did not detect added relation: {plan:#?}"
    );
    assert!(
        plan.steps.iter().any(
            |step| matches!(step, dinoco_cli::sql::MigrationStep::DropForeignKey(item) if item.table == "migration_detect_old_rel")
        ),
        "plan did not detect removed relation: {plan:#?}"
    );
    assert!(
        plan.warnings.iter().any(|warning| warning.destructive && warning.message.contains("data will be lost")),
        "plan did not flag destructive data loss: {plan:#?}"
    );

    Ok(())
}

async fn run_all_methods(client: DinocoClient) -> anyhow::Result<()> {
    let mut user = User::new("a@dinoco.rs".to_string(), "admin".to_string(), 21);
    user.tokens = vec![Token::new(), Token::new()];
    insert_into::<User>().values(&user).execute(&client).await?;

    let returned = insert_into::<User>()
        .values(User::new("returning@dinoco.rs".to_string(), "admin".to_string(), 25))
        .returning::<UserSelect>()
        .execute(&client)
        .await?;
    assert_eq!(returned.email, "returning@dinoco.rs");

    let returned_many = insert_many::<User>()
        .values(vec![
            User::new("many-a@dinoco.rs".to_string(), "admin".to_string(), 30),
            User::new("many-b@dinoco.rs".to_string(), "admin".to_string(), 40),
        ])
        .returning::<UserSelect>()
        .execute(&client)
        .await?;
    assert_eq!(returned_many.len(), 2);

    let users = find_many::<User>()
        .includes(|x| x.tokens().order_by(|token| token.id.asc()).take(10).skip(0))
        .order_by(|x| x.email.asc())
        .read_in_primary()
        .execute(&client)
        .await?;
    assert_eq!(users.iter().find(|item| item.email == "a@dinoco.rs").expect("nested user").tokens.len(), 2);

    assert_eq!(find_many::<User>().where_(|x| x.email.like("dinoco")).execute(&client).await?.len(), 4);
    assert_eq!(find_many::<User>().where_(|x| x.email.starts_with("many")).execute(&client).await?.len(), 2);
    assert_eq!(
        find_first::<User>().where_(|x| x.email.ends_with("@dinoco.rs")).execute(&client).await?.unwrap().age,
        21
    );
    assert_eq!(find_many::<User>().where_(|x| x.age.between(20, 30)).execute(&client).await?.len(), 3);

    let selected = find_first::<User>()
        .select::<UserSelect>()
        .where_(|x| x.email.eq("a@dinoco.rs"))
        .read_in_primary()
        .execute(&client)
        .await?
        .expect("selected user");
    assert_eq!(selected.email, "a@dinoco.rs");

    let token = find_first::<Token>()
        .includes(|x| x.user())
        .where_(|x| x.user_id.not_null())
        .execute(&client)
        .await?
        .expect("token");
    assert!(token.user.is_some());

    dinoco::update::<User>()
        .where_(|x| x.email.eq("a@dinoco.rs"))
        .update(|x| x.email.set("b@dinoco.rs".to_string()))
        .execute(&client)
        .await?;

    let updated_rows = dinoco::update::<User>()
        .where_(|x| x.email.eq("b@dinoco.rs"))
        .update(|x| x.office.set("member".to_string()))
        .returning::<UserSelect>()
        .execute(&client)
        .await?;
    assert_eq!(updated_rows.len(), 1);

    let updated = dinoco::find_and_update::<User>()
        .where_(|x| x.email.eq("b@dinoco.rs"))
        .update(|x| x.office.set("owner".to_string()))
        .execute(&client)
        .await?;
    assert_eq!(updated.office, "owner");

    dinoco::update_many::<User>()
        .where_(|x| x.office.eq("admin"))
        .update(|x| x.office.set("staff".to_string()))
        .execute(&client)
        .await?;
    let staff = dinoco::update_many::<User>()
        .where_(|x| x.office.eq("staff"))
        .update(|x| x.office.set("manager".to_string()))
        .returning::<UserSelect>()
        .execute(&client)
        .await?;
    assert_eq!(staff.len(), 3);

    dinoco::update::<UserPost>()
        .where_(|x| x.user_id.eq("user-a"))
        .update(|x| x.post_id.connect("post-a"))
        .execute(&client)
        .await?;
    assert_eq!(dinoco::count::<UserPost>().execute(&client).await?.total, 1);
    dinoco::update_many::<UserPost>()
        .where_(|x| x.user_id.eq("user-a"))
        .update(|x| x.post_id.disconnect("post-a"))
        .execute(&client)
        .await?;
    assert_eq!(dinoco::count::<UserPost>().execute(&client).await?.total, 0);

    let count = dinoco::count::<User>()
        .includes(|x| x.tokens().where_(|token| token.is_expired.eq(false)))
        .execute(&client)
        .await?;
    assert_eq!(count.total, 4);
    assert_eq!(count.tokens.expect("token count").total, 2);

    let deleted = dinoco::delete::<User>()
        .where_(|x| x.email.eq("returning@dinoco.rs"))
        .returning::<UserSelect>()
        .execute(&client)
        .await?;
    assert_eq!(deleted.len(), 1);

    dinoco::delete::<User>().where_(|x| x.email.eq("many-b@dinoco.rs")).execute(&client).await?;
    assert!(find_first::<User>().where_(|x| x.email.eq("many-b@dinoco.rs")).execute(&client).await?.is_none());

    let deleted_tokens =
        dinoco::delete_many::<Token>().where_(|x| x.user_id.not_null()).returning::<Token>().execute(&client).await?;
    assert_eq!(deleted_tokens.len(), 2);
    dinoco::delete_many::<User>().where_(|x| x.office.batch(vec!["manager", "owner"])).execute(&client).await?;
    assert_eq!(dinoco::count::<User>().execute(&client).await?.total, 0);

    Ok(())
}

fn monotonic() -> u128 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
}
