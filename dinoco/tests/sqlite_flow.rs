use std::env;

use dinoco::{
    DinocoAdapter, DinocoClient, DinocoError, DinocoResult, DinocoValue, InsertModel, InsertRelation, Model,
    Projection, Rowable, ScalarField, UpdateModel, delete, delete_many, find_first, find_many, insert_into,
    insert_many, update, update_many,
};
use uuid::Uuid;

#[derive(Debug, Clone, Rowable)]
struct User {
    id: i64,
    name: String,
    email: String,
    active: bool,
}

struct UserWhere {
    id: ScalarField<i64>,
    name: ScalarField<String>,
    email: ScalarField<String>,
    active: ScalarField<bool>,
}

struct UserInclude {}

#[derive(Debug, Clone, Rowable)]
struct Team {
    id: String,
    name: String,
}

struct TeamWhere {
    id: ScalarField<String>,
    name: ScalarField<String>,
}

struct TeamInclude {}

#[derive(Debug, Clone, Rowable)]
struct Member {
    id: String,
    name: String,
    teamId: String,
}

struct MemberWhere {
    id: ScalarField<String>,
    name: ScalarField<String>,
    teamId: ScalarField<String>,
}

struct MemberInclude {}

fn sqlite_url(name: &str) -> String {
    let mut path = env::temp_dir();

    path.push(format!("dinoco-tests-{name}-{}.sqlite", Uuid::now_v7()));

    format!("file:{}", path.display())
}

async fn create_users_table(client: &DinocoClient<dinoco_engine::SqliteAdapter>) {
    client
        .primary()
        .execute(
            r#"CREATE TABLE "users" (
                "id" INTEGER PRIMARY KEY,
                "name" TEXT NOT NULL,
                "email" TEXT NOT NULL UNIQUE,
                "active" BOOLEAN NOT NULL
            )"#,
            &[],
        )
        .await
        .expect("users table should be created");
}

async fn create_team_tables(client: &DinocoClient<dinoco_engine::SqliteAdapter>) {
    client
        .primary()
        .execute(
            r#"CREATE TABLE "teams" (
                "id" TEXT PRIMARY KEY,
                "name" TEXT NOT NULL
            )"#,
            &[],
        )
        .await
        .expect("teams table should be created");

    client
        .primary()
        .execute(
            r#"CREATE TABLE "members" (
                "id" TEXT PRIMARY KEY,
                "name" TEXT NOT NULL,
                "teamId" TEXT NOT NULL
            )"#,
            &[],
        )
        .await
        .expect("members table should be created");
}

#[tokio::test]
async fn sqlite_crud_flow_and_delete_many_work() -> DinocoResult<()> {
    let client = DinocoClient::<dinoco_engine::SqliteAdapter>::new(sqlite_url("crud"), vec![]).await?;

    create_users_table(&client).await;

    insert_into::<User>()
        .values(User { id: 1, name: "Matheus".to_string(), email: "matheus@dinoco.dev".to_string(), active: true })
        .execute(&client)
        .await?;

    insert_many::<User>()
        .values(vec![
            User { id: 2, name: "Ana".to_string(), email: "ana@dinoco.dev".to_string(), active: true },
            User { id: 3, name: "Caio".to_string(), email: "caio@dinoco.dev".to_string(), active: false },
        ])
        .execute(&client)
        .await?;

    let first = find_first::<User>()
        .cond(|x| x.email.eq("matheus@dinoco.dev"))
        .execute(&client)
        .await?
        .expect("user should exist");

    assert_eq!(first.id, 1);

    let ordered = find_many::<User>().order_by(|x| x.id.desc()).execute(&client).await?;

    assert_eq!(ordered.iter().map(|item| item.id).collect::<Vec<_>>(), vec![3, 2, 1]);

    update::<User>()
        .cond(|x| x.id.eq(1_i64))
        .values(User {
            id: 1,
            name: "Matheus Updated".to_string(),
            email: "updated@dinoco.dev".to_string(),
            active: true,
        })
        .execute(&client)
        .await?;

    update_many::<User>()
        .values(vec![
            User { id: 2, name: "Ana Batch".to_string(), email: "ana-batch@dinoco.dev".to_string(), active: true },
            User { id: 3, name: "Caio Batch".to_string(), email: "caio-batch@dinoco.dev".to_string(), active: false },
        ])
        .execute(&client)
        .await?;

    delete::<User>().cond(|x| x.id.eq(1_i64)).execute(&client).await?;

    let remaining = find_many::<User>().order_by(|x| x.id.asc()).execute(&client).await?;

    assert_eq!(remaining.iter().map(|item| item.id).collect::<Vec<_>>(), vec![2, 3]);
    assert_eq!(remaining[0].name, "Ana Batch");

    delete_many::<User>().execute(&client).await?;

    let empty = find_many::<User>().execute(&client).await?;

    assert!(empty.is_empty());

    Ok(())
}

#[tokio::test]
async fn insert_validation_rejects_empty_required_fields() -> DinocoResult<()> {
    let client = DinocoClient::<dinoco_engine::SqliteAdapter>::new(sqlite_url("validation"), vec![]).await?;

    create_users_table(&client).await;

    let error = insert_into::<User>()
        .values(User { id: 1, name: "".to_string(), email: "   ".to_string(), active: true })
        .execute(&client)
        .await
        .expect_err("insert should fail validation");

    match error {
        DinocoError::ParseError(message) => {
            assert!(message.contains("User.name"));
        }
        other => panic!("expected parse error, got {other:?}"),
    }

    Ok(())
}

#[tokio::test]
async fn relation_insert_binds_foreign_keys_for_single_and_many() -> DinocoResult<()> {
    let client = DinocoClient::<dinoco_engine::SqliteAdapter>::new(sqlite_url("relations"), vec![]).await?;

    create_team_tables(&client).await;

    insert_into::<Team>()
        .values(Team { id: "team-1".to_string(), name: "Dinoco".to_string() })
        .with_relation(Member { id: "member-1".to_string(), name: "Matheus".to_string(), teamId: String::new() })
        .execute(&client)
        .await?;

    insert_many::<Team>()
        .values(vec![
            Team { id: "team-2".to_string(), name: "Platform".to_string() },
            Team { id: "team-3".to_string(), name: "Compiler".to_string() },
        ])
        .with_relation(vec![
            Member { id: "member-2".to_string(), name: "Ana".to_string(), teamId: String::new() },
            Member { id: "member-3".to_string(), name: "Caio".to_string(), teamId: String::new() },
        ])
        .execute(&client)
        .await?;

    let members = find_many::<Member>().order_by(|x| x.id.asc()).execute(&client).await?;

    assert_eq!(
        members.iter().map(|item| (&item.id, &item.teamId)).collect::<Vec<_>>(),
        vec![
            (&"member-1".to_string(), &"team-1".to_string()),
            (&"member-2".to_string(), &"team-2".to_string()),
            (&"member-3".to_string(), &"team-3".to_string()),
        ]
    );

    Ok(())
}

impl Projection<User> for User {
    fn columns() -> &'static [&'static str] {
        &["id", "name", "email", "active"]
    }
}

impl Projection<Team> for Team {
    fn columns() -> &'static [&'static str] {
        &["id", "name"]
    }
}

impl Projection<Member> for Member {
    fn columns() -> &'static [&'static str] {
        &["id", "name", "teamId"]
    }
}

impl InsertModel for User {
    fn insert_columns() -> &'static [&'static str] {
        &["id", "name", "email", "active"]
    }

    fn into_insert_row(self) -> Vec<DinocoValue> {
        vec![self.id.into(), self.name.into(), self.email.into(), self.active.into()]
    }

    fn validate_insert(&self) -> DinocoResult<()> {
        if self.name.trim().is_empty() {
            return Err(DinocoError::ParseError(
                "Field 'User.name' is required for insert and cannot be empty".to_string(),
            ));
        }

        if self.email.trim().is_empty() {
            return Err(DinocoError::ParseError(
                "Field 'User.email' is required for insert and cannot be empty".to_string(),
            ));
        }

        Ok(())
    }
}

impl InsertModel for Team {
    fn insert_columns() -> &'static [&'static str] {
        &["id", "name"]
    }

    fn into_insert_row(self) -> Vec<DinocoValue> {
        vec![self.id.into(), self.name.into()]
    }
}

impl InsertModel for Member {
    fn insert_columns() -> &'static [&'static str] {
        &["id", "name", "teamId"]
    }

    fn into_insert_row(self) -> Vec<DinocoValue> {
        vec![self.id.into(), self.name.into(), self.teamId.into()]
    }
}

impl InsertRelation<Member> for Team {
    fn bind_relation(&self, item: &mut Member) {
        item.teamId = self.id.clone();
    }
}

impl UpdateModel for User {
    fn update_columns() -> &'static [&'static str] {
        &["name", "email", "active"]
    }

    fn into_update_row(self) -> Vec<DinocoValue> {
        vec![self.name.into(), self.email.into(), self.active.into()]
    }

    fn update_identity_conditions(&self) -> Vec<dinoco_engine::Expression> {
        vec![dinoco_engine::Expression::Column("id".to_string()).eq(self.id)]
    }
}

impl Model for User {
    type Include = UserInclude;
    type Where = UserWhere;

    fn table_name() -> &'static str {
        "users"
    }
}

impl Model for Team {
    type Include = TeamInclude;
    type Where = TeamWhere;

    fn table_name() -> &'static str {
        "teams"
    }
}

impl Model for Member {
    type Include = MemberInclude;
    type Where = MemberWhere;

    fn table_name() -> &'static str {
        "members"
    }
}

impl Default for UserWhere {
    fn default() -> Self {
        Self {
            id: ScalarField::new("id"),
            name: ScalarField::new("name"),
            email: ScalarField::new("email"),
            active: ScalarField::new("active"),
        }
    }
}

impl Default for UserInclude {
    fn default() -> Self {
        Self {}
    }
}

impl Default for TeamWhere {
    fn default() -> Self {
        Self { id: ScalarField::new("id"), name: ScalarField::new("name") }
    }
}

impl Default for TeamInclude {
    fn default() -> Self {
        Self {}
    }
}

impl Default for MemberWhere {
    fn default() -> Self {
        Self { id: ScalarField::new("id"), name: ScalarField::new("name"), teamId: ScalarField::new("teamId") }
    }
}

impl Default for MemberInclude {
    fn default() -> Self {
        Self {}
    }
}
