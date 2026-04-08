use std::env;

use dinoco::{
    DinocoAdapter, DinocoClient, DinocoError, DinocoResult, DinocoValue, InsertConnection, InsertModel, InsertRelation,
    Model, Projection, RelationLinkPlan, RelationMutationModel, RelationMutationWhere, RelationScalarField,
    RelationWritePlan, Rowable, ScalarField, UpdateModel, count, delete, delete_many, find_first, find_many,
    insert_into, insert_many, update, update_many,
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

#[derive(Debug, Clone, Rowable)]
struct Article {
    id: String,
    title: String,
}

struct ArticleWhere {
    id: ScalarField<String>,
    title: ScalarField<String>,
}

struct ArticleInclude {}

struct ArticleRelations {}

#[derive(Debug, Clone, Rowable)]
struct Label {
    id: String,
    name: String,
}

struct LabelWhere {
    id: ScalarField<String>,
    name: ScalarField<String>,
}

struct LabelRelationWhere {
    id: RelationScalarField<String>,
    name: RelationScalarField<String>,
}

struct LabelInclude {}

#[derive(Debug, Clone, Rowable)]
struct ArticleLabel {
    article_id: String,
    label_id: String,
}

struct ArticleLabelWhere {
    article_id: ScalarField<String>,
    label_id: ScalarField<String>,
}

struct ArticleLabelInclude {}

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

async fn create_article_tables(client: &DinocoClient<dinoco_engine::SqliteAdapter>) {
    client
        .primary()
        .execute(
            r#"CREATE TABLE "articles" (
                "id" TEXT PRIMARY KEY,
                "title" TEXT NOT NULL
            )"#,
            &[],
        )
        .await
        .expect("articles table should be created");

    client
        .primary()
        .execute(
            r#"CREATE TABLE "labels" (
                "id" TEXT PRIMARY KEY,
                "name" TEXT NOT NULL
            )"#,
            &[],
        )
        .await
        .expect("labels table should be created");

    client
        .primary()
        .execute(
            r#"CREATE TABLE "_ArticleLabels" (
                "article_id" TEXT NOT NULL,
                "label_id" TEXT NOT NULL,
                PRIMARY KEY ("article_id", "label_id")
            )"#,
            &[],
        )
        .await
        .expect("article labels table should be created");
}

#[tokio::test]
async fn sqlite_crud_flow_and_delete_many_work() -> DinocoResult<()> {
    let client = DinocoClient::<dinoco_engine::SqliteAdapter>::new(
        sqlite_url("crud"),
        vec![],
        dinoco::DinocoClientConfig::default(),
    )
    .await?;

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

    let active_count = count::<User>().cond(|x| x.active.eq(true)).execute(&client).await?;

    assert_eq!(active_count, 2);

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
    let client = DinocoClient::<dinoco_engine::SqliteAdapter>::new(
        sqlite_url("validation"),
        vec![],
        dinoco::DinocoClientConfig::default(),
    )
    .await?;

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
    let client = DinocoClient::<dinoco_engine::SqliteAdapter>::new(
        sqlite_url("relations"),
        vec![],
        dinoco::DinocoClientConfig::default(),
    )
    .await?;

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

#[tokio::test]
async fn many_to_many_insert_with_relation_and_update_connect_disconnect_work() -> DinocoResult<()> {
    let client = DinocoClient::<dinoco_engine::SqliteAdapter>::new(
        sqlite_url("many-to-many-relations"),
        vec![],
        dinoco::DinocoClientConfig::default(),
    )
    .await?;

    create_article_tables(&client).await;

    insert_into::<Article>()
        .values(Article { id: "article-1".to_string(), title: "Dinoco Connect".to_string() })
        .with_relation(Label { id: "label-1".to_string(), name: "orm".to_string() })
        .execute(&client)
        .await?;

    insert_many::<Article>()
        .values(vec![
            Article { id: "article-2".to_string(), title: "Dinoco Insert Many".to_string() },
            Article { id: "article-3".to_string(), title: "Dinoco Disconnect".to_string() },
        ])
        .with_relation(vec![
            Label { id: "label-2".to_string(), name: "rust".to_string() },
            Label { id: "label-3".to_string(), name: "sqlite".to_string() },
        ])
        .execute(&client)
        .await?;

    update::<Article>()
        .cond(|x| x.id.eq("article-1"))
        .connect(|x| x.labels().id.eq("label-2"))
        .execute(&client)
        .await?;

    update::<Article>()
        .cond(|x| x.id.eq("article-1"))
        .disconnect(|x| x.labels().id.eq("label-1"))
        .execute(&client)
        .await?;

    let rows = find_many::<ArticleLabel>().order_by(|x| x.article_id.asc()).execute(&client).await?;

    assert_eq!(
        rows.iter().map(|row| (&row.article_id, &row.label_id)).collect::<Vec<_>>(),
        vec![
            (&"article-1".to_string(), &"label-2".to_string()),
            (&"article-2".to_string(), &"label-2".to_string()),
            (&"article-3".to_string(), &"label-3".to_string()),
        ]
    );

    Ok(())
}

#[tokio::test]
async fn insert_with_connection_links_existing_relations() -> DinocoResult<()> {
    let client = DinocoClient::<dinoco_engine::SqliteAdapter>::new(
        sqlite_url("insert-connections"),
        vec![],
        dinoco::DinocoClientConfig::default(),
    )
    .await?;

    create_team_tables(&client).await;
    create_article_tables(&client).await;

    insert_many::<Member>()
        .values(vec![
            Member { id: "member-10".to_string(), name: "Julia".to_string(), teamId: "legacy".to_string() },
            Member { id: "member-11".to_string(), name: "Rafa".to_string(), teamId: "legacy".to_string() },
            Member { id: "member-12".to_string(), name: "Bia".to_string(), teamId: "legacy".to_string() },
        ])
        .execute(&client)
        .await?;

    insert_many::<Label>()
        .values(vec![
            Label { id: "label-10".to_string(), name: "backend".to_string() },
            Label { id: "label-11".to_string(), name: "orm".to_string() },
            Label { id: "label-12".to_string(), name: "rust".to_string() },
        ])
        .execute(&client)
        .await?;

    insert_into::<Team>()
        .values(Team { id: "team-10".to_string(), name: "Infra".to_string() })
        .with_connection(Member {
            id: "member-10".to_string(),
            name: "Julia".to_string(),
            teamId: "legacy".to_string(),
        })
        .execute(&client)
        .await?;

    insert_many::<Team>()
        .values(vec![
            Team { id: "team-11".to_string(), name: "Data".to_string() },
            Team { id: "team-12".to_string(), name: "DX".to_string() },
        ])
        .with_connections(vec![
            vec![Member { id: "member-11".to_string(), name: "Rafa".to_string(), teamId: "legacy".to_string() }],
            vec![Member { id: "member-12".to_string(), name: "Bia".to_string(), teamId: "legacy".to_string() }],
        ])
        .execute(&client)
        .await?;

    let members = find_many::<Member>().order_by(|x| x.id.asc()).execute(&client).await?;

    assert_eq!(
        members.iter().map(|item| (&item.id, &item.teamId)).collect::<Vec<_>>(),
        vec![
            (&"member-10".to_string(), &"team-10".to_string()),
            (&"member-11".to_string(), &"team-11".to_string()),
            (&"member-12".to_string(), &"team-12".to_string()),
        ]
    );

    insert_into::<Article>()
        .values(Article { id: "article-10".to_string(), title: "Connect Existing".to_string() })
        .with_connection(Label { id: "label-10".to_string(), name: "backend".to_string() })
        .execute(&client)
        .await?;

    insert_many::<Article>()
        .values(vec![
            Article { id: "article-11".to_string(), title: "Connect Multiple".to_string() },
            Article { id: "article-12".to_string(), title: "Connect Batch".to_string() },
        ])
        .with_connections(vec![
            vec![
                Label { id: "label-11".to_string(), name: "orm".to_string() },
                Label { id: "label-12".to_string(), name: "rust".to_string() },
            ],
            vec![Label { id: "label-10".to_string(), name: "backend".to_string() }],
        ])
        .execute(&client)
        .await?;

    let rows = find_many::<ArticleLabel>().order_by(|x| x.article_id.asc()).execute(&client).await?;

    assert_eq!(
        rows.iter().map(|row| (&row.article_id, &row.label_id)).collect::<Vec<_>>(),
        vec![
            (&"article-10".to_string(), &"label-10".to_string()),
            (&"article-11".to_string(), &"label-11".to_string()),
            (&"article-11".to_string(), &"label-12".to_string()),
            (&"article-12".to_string(), &"label-10".to_string()),
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

impl Projection<Article> for Article {
    fn columns() -> &'static [&'static str] {
        &["id", "title"]
    }
}

impl Projection<Label> for Label {
    fn columns() -> &'static [&'static str] {
        &["id", "name"]
    }
}

impl Projection<ArticleLabel> for ArticleLabel {
    fn columns() -> &'static [&'static str] {
        &["article_id", "label_id"]
    }
}

impl InsertModel for User {
    fn insert_columns() -> &'static [&'static str] {
        &["id", "name", "email", "active"]
    }

    fn into_insert_row(self) -> Vec<DinocoValue> {
        vec![self.id.into(), self.name.into(), self.email.into(), self.active.into()]
    }

    fn insert_identity_conditions(&self) -> Vec<dinoco_engine::Expression> {
        vec![dinoco_engine::Expression::Column("id".to_string()).eq(self.id)]
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

    fn insert_identity_conditions(&self) -> Vec<dinoco_engine::Expression> {
        vec![dinoco_engine::Expression::Column("id".to_string()).eq(self.id.clone())]
    }
}

impl InsertModel for Member {
    fn insert_columns() -> &'static [&'static str] {
        &["id", "name", "teamId"]
    }

    fn into_insert_row(self) -> Vec<DinocoValue> {
        vec![self.id.into(), self.name.into(), self.teamId.into()]
    }

    fn insert_identity_conditions(&self) -> Vec<dinoco_engine::Expression> {
        vec![dinoco_engine::Expression::Column("id".to_string()).eq(self.id.clone())]
    }
}

impl InsertModel for Article {
    fn insert_columns() -> &'static [&'static str] {
        &["id", "title"]
    }

    fn into_insert_row(self) -> Vec<DinocoValue> {
        vec![self.id.into(), self.title.into()]
    }

    fn insert_identity_conditions(&self) -> Vec<dinoco_engine::Expression> {
        vec![dinoco_engine::Expression::Column("id".to_string()).eq(self.id.clone())]
    }
}

impl InsertModel for Label {
    fn insert_columns() -> &'static [&'static str] {
        &["id", "name"]
    }

    fn into_insert_row(self) -> Vec<DinocoValue> {
        vec![self.id.into(), self.name.into()]
    }

    fn insert_identity_conditions(&self) -> Vec<dinoco_engine::Expression> {
        vec![dinoco_engine::Expression::Column("id".to_string()).eq(self.id.clone())]
    }
}

impl InsertModel for ArticleLabel {
    fn insert_columns() -> &'static [&'static str] {
        &["article_id", "label_id"]
    }

    fn into_insert_row(self) -> Vec<DinocoValue> {
        vec![self.article_id.into(), self.label_id.into()]
    }

    fn insert_identity_conditions(&self) -> Vec<dinoco_engine::Expression> {
        vec![
            dinoco_engine::Expression::Column("article_id".to_string()).eq(self.article_id.clone()),
            dinoco_engine::Expression::Column("label_id".to_string()).eq(self.label_id.clone()),
        ]
    }
}

impl InsertRelation<Member> for Team {
    fn bind_relation(&self, item: &mut Member) {
        item.teamId = self.id.clone();
    }
}

impl InsertConnection<Member> for Team {
    fn connection_updates(&self, item: &Member) -> Vec<dinoco::ConnectionUpdatePlan> {
        vec![dinoco::ConnectionUpdatePlan {
            table_name: "members",
            columns: &["name", "teamId"],
            row: vec![item.name.clone().into(), self.id.clone().into()],
            conditions: vec![dinoco_engine::Expression::Column("id".to_string()).eq(item.id.clone())],
        }]
    }
}

impl InsertRelation<Label> for Article {
    fn relation_links(&self, item: &Label) -> Vec<RelationLinkPlan> {
        vec![RelationLinkPlan {
            table_name: "_ArticleLabels",
            columns: &["article_id", "label_id"],
            row: vec![self.id.clone().into(), item.id.clone().into()],
        }]
    }
}

impl InsertConnection<Label> for Article {}

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

impl UpdateModel for Article {
    fn update_columns() -> &'static [&'static str] {
        &["title"]
    }

    fn into_update_row(self) -> Vec<DinocoValue> {
        vec![self.title.into()]
    }

    fn update_identity_conditions(&self) -> Vec<dinoco_engine::Expression> {
        vec![dinoco_engine::Expression::Column("id".to_string()).eq(self.id.clone())]
    }
}

impl UpdateModel for Label {
    fn update_columns() -> &'static [&'static str] {
        &["name"]
    }

    fn into_update_row(self) -> Vec<DinocoValue> {
        vec![self.name.into()]
    }

    fn update_identity_conditions(&self) -> Vec<dinoco_engine::Expression> {
        vec![dinoco_engine::Expression::Column("id".to_string()).eq(self.id.clone())]
    }
}

impl UpdateModel for Member {
    fn update_columns() -> &'static [&'static str] {
        &["name", "teamId"]
    }

    fn into_update_row(self) -> Vec<DinocoValue> {
        vec![self.name.into(), self.teamId.into()]
    }

    fn update_identity_conditions(&self) -> Vec<dinoco_engine::Expression> {
        vec![dinoco_engine::Expression::Column("id".to_string()).eq(self.id.clone())]
    }
}

impl UpdateModel for ArticleLabel {
    fn update_columns() -> &'static [&'static str] {
        &[]
    }

    fn into_update_row(self) -> Vec<DinocoValue> {
        vec![]
    }

    fn update_identity_conditions(&self) -> Vec<dinoco_engine::Expression> {
        vec![
            dinoco_engine::Expression::Column("article_id".to_string()).eq(self.article_id.clone()),
            dinoco_engine::Expression::Column("label_id".to_string()).eq(self.label_id.clone()),
        ]
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

impl Model for Article {
    type Include = ArticleInclude;
    type Where = ArticleWhere;

    fn table_name() -> &'static str {
        "articles"
    }
}

impl Model for Label {
    type Include = LabelInclude;
    type Where = LabelWhere;

    fn table_name() -> &'static str {
        "labels"
    }
}

impl Model for ArticleLabel {
    type Include = ArticleLabelInclude;
    type Where = ArticleLabelWhere;

    fn table_name() -> &'static str {
        "_ArticleLabels"
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

impl Default for ArticleWhere {
    fn default() -> Self {
        Self { id: ScalarField::new("id"), title: ScalarField::new("title") }
    }
}

impl Default for LabelWhere {
    fn default() -> Self {
        Self { id: ScalarField::new("id"), name: ScalarField::new("name") }
    }
}

impl LabelRelationWhere {
    fn new(relation_name: &'static str) -> RelationMutationWhere<Self> {
        RelationMutationWhere::new(Self {
            id: RelationScalarField::new(relation_name, "id"),
            name: RelationScalarField::new(relation_name, "name"),
        })
    }
}

impl Default for ArticleLabelWhere {
    fn default() -> Self {
        Self { article_id: ScalarField::new("article_id"), label_id: ScalarField::new("label_id") }
    }
}

impl Default for MemberInclude {
    fn default() -> Self {
        Self {}
    }
}

impl Default for ArticleInclude {
    fn default() -> Self {
        Self {}
    }
}

impl Default for ArticleRelations {
    fn default() -> Self {
        Self {}
    }
}

impl ArticleRelations {
    fn labels(&self) -> RelationMutationWhere<LabelRelationWhere> {
        LabelRelationWhere::new("labels")
    }
}

impl Default for LabelInclude {
    fn default() -> Self {
        Self {}
    }
}

impl Default for ArticleLabelInclude {
    fn default() -> Self {
        Self {}
    }
}

impl RelationMutationModel for Article {
    type Relations = ArticleRelations;

    fn relation_write_plan(target: dinoco::RelationMutationTarget) -> Option<RelationWritePlan> {
        match target.relation_name {
            "labels" => Some(RelationWritePlan {
                join_table_name: "_ArticleLabels",
                source_table_name: "articles",
                source_key_column: "id",
                source_join_column: "article_id",
                target_table_name: "labels",
                target_key_column: "id",
                target_join_column: "label_id",
                target_expression: target.expression,
            }),
            _ => None,
        }
    }
}
