#![allow(non_snake_case)]

use std::collections::HashMap;

use dinoco::{
    DinocoAdapter, DinocoClient, DinocoGenericRow, DinocoResult, DinocoRow, DinocoValue, Extend, IncludeLoaderFuture,
    InsertConnection, InsertModel, InsertRelation, IntoDinocoValue, Model, Projection, RelationField, RelationLinkPlan,
    RelationMutationModel, RelationMutationWhere, RelationScalarField, RelationWritePlan, Rowable, ScalarField,
    UpdateModel, find_first, find_many, insert_into, insert_many, update,
};
use dinoco_engine::{MySqlAdapter, PostgresAdapter, SqliteAdapter};

mod common;

const TEAMS_TABLE: &str = "teams";
const MEMBERS_TABLE: &str = "members";
const ARTICLES_TABLE: &str = "articles";
const LABELS_TABLE: &str = "labels";
const ARTICLE_LABELS_TABLE: &str = "_ArticleLabels";
const USERS_TABLE: &str = "users";
const POSTS_TABLE: &str = "posts";
const COMMENTS_TABLE: &str = "comments";

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
struct AutoTeam {
    id: i64,
    name: String,
}

struct AutoTeamWhere {
    id: ScalarField<i64>,
    name: ScalarField<String>,
}

#[derive(Default)]
struct AutoTeamInclude {}

#[derive(Debug, Clone, Rowable)]
struct AutoMember {
    id: i64,
    name: String,
    teamId: i64,
}

struct AutoMemberWhere {
    id: ScalarField<i64>,
    name: ScalarField<String>,
    teamId: ScalarField<i64>,
}

#[derive(Default)]
struct AutoMemberInclude {}

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

#[derive(Debug, Clone, Rowable)]
struct AutoArticle {
    id: i64,
    title: String,
}

struct AutoArticleWhere {
    id: ScalarField<i64>,
    title: ScalarField<String>,
}

#[derive(Default)]
struct AutoArticleInclude {}

#[derive(Debug, Clone, Rowable)]
struct AutoLabel {
    id: i64,
    name: String,
}

struct AutoLabelWhere {
    id: ScalarField<i64>,
    name: ScalarField<String>,
}

#[derive(Default)]
struct AutoLabelInclude {}

#[derive(Debug, Clone, Rowable)]
struct AutoArticleLabel {
    article_id: i64,
    label_id: i64,
}

struct AutoArticleLabelWhere {
    article_id: ScalarField<i64>,
    label_id: ScalarField<i64>,
}

#[derive(Default)]
struct AutoArticleLabelInclude {}

#[derive(Debug, Clone, Rowable)]
struct User {
    id: i64,
    name: String,
}

struct UserWhere {
    id: ScalarField<i64>,
    name: ScalarField<String>,
}

struct UserInclude {}

#[derive(Debug, Clone, Rowable)]
struct Post {
    id: i64,
    title: String,
    published: bool,
    authorId: i64,
}

struct PostWhere {
    id: ScalarField<i64>,
    title: ScalarField<String>,
    published: ScalarField<bool>,
    authorId: ScalarField<i64>,
}

struct PostInclude {}

#[derive(Debug, Clone, Rowable)]
struct Comment {
    id: i64,
    text: String,
    flagged: bool,
    postId: i64,
}

struct CommentWhere {
    id: ScalarField<i64>,
    text: ScalarField<String>,
    flagged: ScalarField<bool>,
    postId: ScalarField<i64>,
}

struct CommentInclude {}

#[derive(Debug, Clone, Extend)]
#[extend(Comment)]
struct CommentListItem {
    id: i64,
    text: String,
}

#[derive(Debug, Clone, Extend)]
#[extend(Post)]
struct PostListItem {
    id: i64,
    title: String,
    comments_count: usize,
    comments: Vec<CommentListItem>,
}

#[derive(Debug, Clone, Extend)]
#[extend(User)]
struct UserListItem {
    id: i64,
    name: String,
    posts_count: usize,
    posts: Vec<PostListItem>,
}

#[derive(Debug, Clone, Extend)]
#[extend(Comment)]
#[insertable]
struct CommentInsertItem {
    id: i64,
    text: String,
    flagged: bool,
    postId: i64,
}

#[derive(Debug, Clone, Extend)]
#[extend(Post)]
#[insertable]
struct PostInsertItem {
    id: i64,
    title: String,
    published: bool,
    authorId: i64,
    comments: Vec<CommentInsertItem>,
}

#[derive(Debug, Clone, Extend)]
#[extend(User)]
#[insertable]
struct UserInsertItem {
    id: i64,
    name: String,
    posts: Vec<PostInsertItem>,
}

#[derive(Debug, Clone, Extend)]
#[extend(Article)]
#[insertable]
struct ArticleWithConnectionPayload {
    id: String,
    title: String,
    labels: Vec<ArticleConnection>,
}

#[derive(Debug, Clone)]
enum ArticleConnection {
    Label(String),
}

impl dinoco::InsertConnectionPayload<Article> for ArticleConnection {
    fn relation_links(&self, parent: &Article) -> Vec<dinoco::RelationLinkPlan> {
        match self {
            Self::Label(label_id) => vec![dinoco::RelationLinkPlan {
                table_name: "_ArticleLabels",
                columns: &["article_id", "label_id"],
                row: vec![parent.id.clone().into(), label_id.clone().into()],
            }],
        }
    }
}

#[tokio::test]
async fn sqlite_relation_insert_binds_foreign_keys_for_single_and_many() -> DinocoResult<()> {
    let client = DinocoClient::<SqliteAdapter>::new(
        common::sqlite_url("relations-adapters"),
        vec![],
        dinoco::DinocoClientConfig::default(),
    )
    .await?;

    drop_team_tables_sqlite(&client).await?;
    create_team_tables_sqlite(&client).await?;
    exercise_relation_insert_flow(&client).await
}

#[tokio::test]
async fn postgres_relation_insert_binds_foreign_keys_for_single_and_many() -> DinocoResult<()> {
    if let Err(err) = async {
        let _lock = common::lock_postgres().await;
        let client =
            DinocoClient::<PostgresAdapter>::new(common::postgres_url(), vec![], dinoco::DinocoClientConfig::default())
                .await?;

        drop_team_tables_postgres(&client).await?;
        create_team_tables_postgres(&client).await?;
        exercise_relation_insert_flow(&client).await?;
        drop_team_tables_postgres(&client).await?;

        Ok(())
    }
    .await
    {
        if common::should_skip_external_adapter_test(&err) {
            eprintln!("skipping postgres relations adapter test: {err}");
            return Ok(());
        }

        return Err(err);
    }

    Ok(())
}

#[tokio::test]
async fn mysql_relation_insert_binds_foreign_keys_for_single_and_many() -> DinocoResult<()> {
    if let Err(err) = async {
        let _lock = common::lock_mysql().await;
        let client =
            DinocoClient::<MySqlAdapter>::new(common::mysql_url(), vec![], dinoco::DinocoClientConfig::default())
                .await?;

        drop_team_tables_mysql(&client).await?;
        create_team_tables_mysql(&client).await?;
        exercise_relation_insert_flow(&client).await?;
        drop_team_tables_mysql(&client).await?;

        Ok(())
    }
    .await
    {
        if common::should_skip_external_adapter_test(&err) {
            eprintln!("skipping mysql relations adapter test: {err}");
            return Ok(());
        }

        return Err(err);
    }

    Ok(())
}

#[tokio::test]
async fn postgres_relation_insert_with_autoincrement_binds_foreign_keys_for_single_and_many() -> DinocoResult<()> {
    if let Err(err) = async {
        let _lock = common::lock_postgres().await;
        let client =
            DinocoClient::<PostgresAdapter>::new(common::postgres_url(), vec![], dinoco::DinocoClientConfig::default())
                .await?;

        drop_auto_team_tables_postgres(&client).await?;
        create_auto_team_tables_postgres(&client).await?;
        exercise_relation_insert_flow_autoincrement(&client).await?;
        drop_auto_team_tables_postgres(&client).await?;

        Ok(())
    }
    .await
    {
        if common::should_skip_external_adapter_test(&err) {
            eprintln!("skipping postgres relations autoincrement adapter test: {err}");
            return Ok(());
        }

        return Err(err);
    }

    Ok(())
}

#[tokio::test]
async fn mysql_relation_insert_with_autoincrement_binds_foreign_keys_for_single_and_many() -> DinocoResult<()> {
    if let Err(err) = async {
        let _lock = common::lock_mysql().await;
        let client =
            DinocoClient::<MySqlAdapter>::new(common::mysql_url(), vec![], dinoco::DinocoClientConfig::default())
                .await?;

        drop_auto_team_tables_mysql(&client).await?;
        create_auto_team_tables_mysql(&client).await?;
        exercise_relation_insert_flow_autoincrement(&client).await?;
        drop_auto_team_tables_mysql(&client).await?;

        Ok(())
    }
    .await
    {
        if common::should_skip_external_adapter_test(&err) {
            eprintln!("skipping mysql relations autoincrement adapter test: {err}");
            return Ok(());
        }

        return Err(err);
    }

    Ok(())
}

#[tokio::test]
async fn sqlite_many_to_many_insert_with_relation_and_update_connect_disconnect_work() -> DinocoResult<()> {
    let client = DinocoClient::<SqliteAdapter>::new(
        common::sqlite_url("many-to-many-adapters"),
        vec![],
        dinoco::DinocoClientConfig::default(),
    )
    .await?;

    drop_article_tables_sqlite(&client).await?;
    create_article_tables_sqlite(&client).await?;
    exercise_many_to_many_flow(&client).await
}

#[tokio::test]
async fn postgres_many_to_many_insert_with_relation_and_update_connect_disconnect_work() -> DinocoResult<()> {
    if let Err(err) = async {
        let _lock = common::lock_postgres().await;
        let client =
            DinocoClient::<PostgresAdapter>::new(common::postgres_url(), vec![], dinoco::DinocoClientConfig::default())
                .await?;

        drop_article_tables_postgres(&client).await?;
        create_article_tables_postgres(&client).await?;
        exercise_many_to_many_flow(&client).await?;
        drop_article_tables_postgres(&client).await?;

        Ok(())
    }
    .await
    {
        if common::should_skip_external_adapter_test(&err) {
            eprintln!("skipping postgres many-to-many adapter test: {err}");
            return Ok(());
        }

        return Err(err);
    }

    Ok(())
}

#[tokio::test]
async fn mysql_many_to_many_insert_with_relation_and_update_connect_disconnect_work() -> DinocoResult<()> {
    if let Err(err) = async {
        let _lock = common::lock_mysql().await;
        let client =
            DinocoClient::<MySqlAdapter>::new(common::mysql_url(), vec![], dinoco::DinocoClientConfig::default())
                .await?;

        drop_article_tables_mysql(&client).await?;
        create_article_tables_mysql(&client).await?;
        exercise_many_to_many_flow(&client).await?;
        drop_article_tables_mysql(&client).await?;

        Ok(())
    }
    .await
    {
        if common::should_skip_external_adapter_test(&err) {
            eprintln!("skipping mysql many-to-many adapter test: {err}");
            return Ok(());
        }

        return Err(err);
    }

    Ok(())
}

#[tokio::test]
async fn postgres_many_to_many_insert_with_relation_supports_autoincrement_ids() -> DinocoResult<()> {
    if let Err(err) = async {
        let _lock = common::lock_postgres().await;
        let client =
            DinocoClient::<PostgresAdapter>::new(common::postgres_url(), vec![], dinoco::DinocoClientConfig::default())
                .await?;

        drop_auto_article_tables_postgres(&client).await?;
        create_auto_article_tables_postgres(&client).await?;
        exercise_many_to_many_flow_autoincrement(&client).await?;
        drop_auto_article_tables_postgres(&client).await?;

        Ok(())
    }
    .await
    {
        if common::should_skip_external_adapter_test(&err) {
            eprintln!("skipping postgres many-to-many autoincrement adapter test: {err}");
            return Ok(());
        }

        return Err(err);
    }

    Ok(())
}

#[tokio::test]
async fn mysql_many_to_many_insert_with_relation_supports_autoincrement_ids() -> DinocoResult<()> {
    if let Err(err) = async {
        let _lock = common::lock_mysql().await;
        let client =
            DinocoClient::<MySqlAdapter>::new(common::mysql_url(), vec![], dinoco::DinocoClientConfig::default())
                .await?;

        drop_auto_article_tables_mysql(&client).await?;
        create_auto_article_tables_mysql(&client).await?;
        exercise_many_to_many_flow_autoincrement(&client).await?;
        drop_auto_article_tables_mysql(&client).await?;

        Ok(())
    }
    .await
    {
        if common::should_skip_external_adapter_test(&err) {
            eprintln!("skipping mysql many-to-many autoincrement adapter test: {err}");
            return Ok(());
        }

        return Err(err);
    }

    Ok(())
}

#[tokio::test]
async fn sqlite_insert_with_connection_links_existing_relations() -> DinocoResult<()> {
    let client = DinocoClient::<SqliteAdapter>::new(
        common::sqlite_url("insert-connections-adapters"),
        vec![],
        dinoco::DinocoClientConfig::default(),
    )
    .await?;

    drop_team_tables_sqlite(&client).await?;
    create_team_tables_sqlite(&client).await?;
    drop_article_tables_sqlite(&client).await?;
    create_article_tables_sqlite(&client).await?;
    exercise_insert_connection_flow(&client).await
}

#[tokio::test]
async fn postgres_insert_with_connection_links_existing_relations() -> DinocoResult<()> {
    if let Err(err) = async {
        let _lock = common::lock_postgres().await;
        let client =
            DinocoClient::<PostgresAdapter>::new(common::postgres_url(), vec![], dinoco::DinocoClientConfig::default())
                .await?;

        drop_team_tables_postgres(&client).await?;
        create_team_tables_postgres(&client).await?;
        drop_article_tables_postgres(&client).await?;
        create_article_tables_postgres(&client).await?;
        exercise_insert_connection_flow(&client).await?;
        drop_article_tables_postgres(&client).await?;
        drop_team_tables_postgres(&client).await?;

        Ok(())
    }
    .await
    {
        if common::should_skip_external_adapter_test(&err) {
            eprintln!("skipping postgres insert connection adapter test: {err}");
            return Ok(());
        }

        return Err(err);
    }

    Ok(())
}

#[tokio::test]
async fn mysql_insert_with_connection_links_existing_relations() -> DinocoResult<()> {
    if let Err(err) = async {
        let _lock = common::lock_mysql().await;
        let client =
            DinocoClient::<MySqlAdapter>::new(common::mysql_url(), vec![], dinoco::DinocoClientConfig::default())
                .await?;

        drop_team_tables_mysql(&client).await?;
        create_team_tables_mysql(&client).await?;
        drop_article_tables_mysql(&client).await?;
        create_article_tables_mysql(&client).await?;
        exercise_insert_connection_flow(&client).await?;
        drop_article_tables_mysql(&client).await?;
        drop_team_tables_mysql(&client).await?;

        Ok(())
    }
    .await
    {
        if common::should_skip_external_adapter_test(&err) {
            eprintln!("skipping mysql insert connection adapter test: {err}");
            return Ok(());
        }

        return Err(err);
    }

    Ok(())
}

#[tokio::test]
async fn sqlite_insertable_extend_recursively_inserts_nested_relations() -> DinocoResult<()> {
    let client = DinocoClient::<SqliteAdapter>::new(
        common::sqlite_url("insertable-extend-recursive"),
        vec![],
        dinoco::DinocoClientConfig::default(),
    )
    .await?;

    drop_count_tables_sqlite(&client).await?;
    create_count_tables_sqlite(&client).await?;
    exercise_recursive_insert_payload_flow(&client).await
}

#[tokio::test]
async fn sqlite_insertable_extend_accepts_connection_payloads() -> DinocoResult<()> {
    let client = DinocoClient::<SqliteAdapter>::new(
        common::sqlite_url("insertable-extend-connections-adapters"),
        vec![],
        dinoco::DinocoClientConfig::default(),
    )
    .await?;

    drop_article_tables_sqlite(&client).await?;
    create_article_tables_sqlite(&client).await?;
    exercise_insert_connection_payload_flow(&client).await
}

#[tokio::test]
async fn postgres_insertable_extend_accepts_connection_payloads() -> DinocoResult<()> {
    if let Err(err) = async {
        let _lock = common::lock_postgres().await;
        let client =
            DinocoClient::<PostgresAdapter>::new(common::postgres_url(), vec![], dinoco::DinocoClientConfig::default())
                .await?;

        drop_article_tables_postgres(&client).await?;
        create_article_tables_postgres(&client).await?;
        exercise_insert_connection_payload_flow(&client).await?;
        drop_article_tables_postgres(&client).await?;

        Ok(())
    }
    .await
    {
        if common::should_skip_external_adapter_test(&err) {
            eprintln!("skipping postgres insertable connection payload adapter test: {err}");
            return Ok(());
        }

        return Err(err);
    }

    Ok(())
}

#[tokio::test]
async fn mysql_insertable_extend_accepts_connection_payloads() -> DinocoResult<()> {
    if let Err(err) = async {
        let _lock = common::lock_mysql().await;
        let client =
            DinocoClient::<MySqlAdapter>::new(common::mysql_url(), vec![], dinoco::DinocoClientConfig::default())
                .await?;

        drop_article_tables_mysql(&client).await?;
        create_article_tables_mysql(&client).await?;
        exercise_insert_connection_payload_flow(&client).await?;
        drop_article_tables_mysql(&client).await?;

        Ok(())
    }
    .await
    {
        if common::should_skip_external_adapter_test(&err) {
            eprintln!("skipping mysql insertable connection payload adapter test: {err}");
            return Ok(());
        }

        return Err(err);
    }

    Ok(())
}

#[tokio::test]
async fn postgres_insertable_extend_recursively_inserts_nested_relations() -> DinocoResult<()> {
    if let Err(err) = async {
        let _lock = common::lock_postgres().await;
        let client =
            DinocoClient::<PostgresAdapter>::new(common::postgres_url(), vec![], dinoco::DinocoClientConfig::default())
                .await?;

        drop_count_tables_postgres(&client).await?;
        create_count_tables_postgres(&client).await?;
        exercise_recursive_insert_payload_flow(&client).await?;
        drop_count_tables_postgres(&client).await?;

        Ok(())
    }
    .await
    {
        if common::should_skip_external_adapter_test(&err) {
            eprintln!("skipping postgres recursive insert adapter test: {err}");
            return Ok(());
        }

        return Err(err);
    }

    Ok(())
}

#[tokio::test]
async fn mysql_insertable_extend_recursively_inserts_nested_relations() -> DinocoResult<()> {
    if let Err(err) = async {
        let _lock = common::lock_mysql().await;
        let client =
            DinocoClient::<MySqlAdapter>::new(common::mysql_url(), vec![], dinoco::DinocoClientConfig::default())
                .await?;

        drop_count_tables_mysql(&client).await?;
        create_count_tables_mysql(&client).await?;
        exercise_recursive_insert_payload_flow(&client).await?;
        drop_count_tables_mysql(&client).await?;

        Ok(())
    }
    .await
    {
        if common::should_skip_external_adapter_test(&err) {
            eprintln!("skipping mysql recursive insert adapter test: {err}");
            return Ok(());
        }

        return Err(err);
    }

    Ok(())
}

#[tokio::test]
async fn sqlite_find_many_and_find_first_can_count_relations() -> DinocoResult<()> {
    let client = DinocoClient::<SqliteAdapter>::new(
        common::sqlite_url("relation-counts-adapters"),
        vec![],
        dinoco::DinocoClientConfig::default(),
    )
    .await?;

    drop_count_tables_sqlite(&client).await?;
    create_count_tables_sqlite(&client).await?;
    exercise_relation_count_flow(&client).await
}

#[tokio::test]
async fn postgres_find_many_and_find_first_can_count_relations() -> DinocoResult<()> {
    if let Err(err) = async {
        let _lock = common::lock_postgres().await;
        let client =
            DinocoClient::<PostgresAdapter>::new(common::postgres_url(), vec![], dinoco::DinocoClientConfig::default())
                .await?;

        drop_count_tables_postgres(&client).await?;
        create_count_tables_postgres(&client).await?;
        exercise_relation_count_flow(&client).await?;
        drop_count_tables_postgres(&client).await?;

        Ok(())
    }
    .await
    {
        if common::should_skip_external_adapter_test(&err) {
            eprintln!("skipping postgres relation count adapter test: {err}");
            return Ok(());
        }

        return Err(err);
    }

    Ok(())
}

#[tokio::test]
async fn mysql_find_many_and_find_first_can_count_relations() -> DinocoResult<()> {
    if let Err(err) = async {
        let _lock = common::lock_mysql().await;
        let client =
            DinocoClient::<MySqlAdapter>::new(common::mysql_url(), vec![], dinoco::DinocoClientConfig::default())
                .await?;

        drop_count_tables_mysql(&client).await?;
        create_count_tables_mysql(&client).await?;
        exercise_relation_count_flow(&client).await?;
        drop_count_tables_mysql(&client).await?;

        Ok(())
    }
    .await
    {
        if common::should_skip_external_adapter_test(&err) {
            eprintln!("skipping mysql relation count adapter test: {err}");
            return Ok(());
        }

        return Err(err);
    }

    Ok(())
}

async fn drop_team_tables_sqlite(client: &DinocoClient<SqliteAdapter>) -> DinocoResult<()> {
    client.primary().execute(&format!(r#"DROP TABLE IF EXISTS "{MEMBERS_TABLE}""#), &[]).await?;
    client.primary().execute(&format!(r#"DROP TABLE IF EXISTS "{TEAMS_TABLE}""#), &[]).await
}

async fn create_team_tables_sqlite(client: &DinocoClient<SqliteAdapter>) -> DinocoResult<()> {
    client
        .primary()
        .execute(
            r#"CREATE TABLE "teams" (
                "id" TEXT PRIMARY KEY,
                "name" TEXT NOT NULL
            )"#,
            &[],
        )
        .await?;

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
}

async fn drop_team_tables_postgres(client: &DinocoClient<PostgresAdapter>) -> DinocoResult<()> {
    client.primary().execute(&format!(r#"DROP TABLE IF EXISTS "{MEMBERS_TABLE}""#), &[]).await?;
    client.primary().execute(&format!(r#"DROP TABLE IF EXISTS "{TEAMS_TABLE}""#), &[]).await
}

async fn create_team_tables_postgres(client: &DinocoClient<PostgresAdapter>) -> DinocoResult<()> {
    client
        .primary()
        .execute(
            r#"CREATE TABLE "teams" (
                "id" TEXT PRIMARY KEY,
                "name" TEXT NOT NULL
            )"#,
            &[],
        )
        .await?;

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
}

async fn drop_team_tables_mysql(client: &DinocoClient<MySqlAdapter>) -> DinocoResult<()> {
    client.primary().execute(&format!("DROP TABLE IF EXISTS `{MEMBERS_TABLE}`"), &[]).await?;
    client.primary().execute(&format!("DROP TABLE IF EXISTS `{TEAMS_TABLE}`"), &[]).await
}

async fn create_team_tables_mysql(client: &DinocoClient<MySqlAdapter>) -> DinocoResult<()> {
    client
        .primary()
        .execute("CREATE TABLE `teams` (`id` VARCHAR(255) PRIMARY KEY, `name` VARCHAR(255) NOT NULL)", &[])
        .await?;

    client
        .primary()
        .execute(
            "CREATE TABLE `members` (`id` VARCHAR(255) PRIMARY KEY, `name` VARCHAR(255) NOT NULL, `teamId` VARCHAR(255) NOT NULL)",
            &[],
        )
        .await
}

async fn drop_auto_team_tables_postgres(client: &DinocoClient<PostgresAdapter>) -> DinocoResult<()> {
    client.primary().execute(r#"DROP TABLE IF EXISTS "auto_members""#, &[]).await?;
    client.primary().execute(r#"DROP TABLE IF EXISTS "auto_teams""#, &[]).await
}

async fn create_auto_team_tables_postgres(client: &DinocoClient<PostgresAdapter>) -> DinocoResult<()> {
    client
        .primary()
        .execute(
            r#"CREATE TABLE "auto_teams" (
                "id" BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
                "name" TEXT NOT NULL
            )"#,
            &[],
        )
        .await?;

    client
        .primary()
        .execute(
            r#"CREATE TABLE "auto_members" (
                "id" BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
                "name" TEXT NOT NULL,
                "teamId" BIGINT NOT NULL
            )"#,
            &[],
        )
        .await
}

async fn drop_auto_team_tables_mysql(client: &DinocoClient<MySqlAdapter>) -> DinocoResult<()> {
    client.primary().execute("DROP TABLE IF EXISTS `auto_members`", &[]).await?;
    client.primary().execute("DROP TABLE IF EXISTS `auto_teams`", &[]).await
}

async fn create_auto_team_tables_mysql(client: &DinocoClient<MySqlAdapter>) -> DinocoResult<()> {
    client
        .primary()
        .execute(
            "CREATE TABLE `auto_teams` (`id` BIGINT AUTO_INCREMENT PRIMARY KEY, `name` VARCHAR(255) NOT NULL)",
            &[],
        )
        .await?;

    client
        .primary()
        .execute(
            "CREATE TABLE `auto_members` (`id` BIGINT AUTO_INCREMENT PRIMARY KEY, `name` VARCHAR(255) NOT NULL, `teamId` BIGINT NOT NULL)",
            &[],
        )
        .await
}

async fn drop_article_tables_sqlite(client: &DinocoClient<SqliteAdapter>) -> DinocoResult<()> {
    client.primary().execute(&format!(r#"DROP TABLE IF EXISTS "{ARTICLE_LABELS_TABLE}""#), &[]).await?;
    client.primary().execute(&format!(r#"DROP TABLE IF EXISTS "{LABELS_TABLE}""#), &[]).await?;
    client.primary().execute(&format!(r#"DROP TABLE IF EXISTS "{ARTICLES_TABLE}""#), &[]).await
}

async fn create_article_tables_sqlite(client: &DinocoClient<SqliteAdapter>) -> DinocoResult<()> {
    client.primary().execute(r#"CREATE TABLE "articles" ("id" TEXT PRIMARY KEY, "title" TEXT NOT NULL)"#, &[]).await?;
    client.primary().execute(r#"CREATE TABLE "labels" ("id" TEXT PRIMARY KEY, "name" TEXT NOT NULL)"#, &[]).await?;
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
}

async fn drop_article_tables_postgres(client: &DinocoClient<PostgresAdapter>) -> DinocoResult<()> {
    client.primary().execute(&format!(r#"DROP TABLE IF EXISTS "{ARTICLE_LABELS_TABLE}""#), &[]).await?;
    client.primary().execute(&format!(r#"DROP TABLE IF EXISTS "{LABELS_TABLE}""#), &[]).await?;
    client.primary().execute(&format!(r#"DROP TABLE IF EXISTS "{ARTICLES_TABLE}""#), &[]).await
}

async fn create_article_tables_postgres(client: &DinocoClient<PostgresAdapter>) -> DinocoResult<()> {
    client.primary().execute(r#"CREATE TABLE "articles" ("id" TEXT PRIMARY KEY, "title" TEXT NOT NULL)"#, &[]).await?;
    client.primary().execute(r#"CREATE TABLE "labels" ("id" TEXT PRIMARY KEY, "name" TEXT NOT NULL)"#, &[]).await?;
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
}

async fn drop_article_tables_mysql(client: &DinocoClient<MySqlAdapter>) -> DinocoResult<()> {
    client.primary().execute(&format!("DROP TABLE IF EXISTS `{ARTICLE_LABELS_TABLE}`"), &[]).await?;
    client.primary().execute(&format!("DROP TABLE IF EXISTS `{LABELS_TABLE}`"), &[]).await?;
    client.primary().execute(&format!("DROP TABLE IF EXISTS `{ARTICLES_TABLE}`"), &[]).await
}

async fn create_article_tables_mysql(client: &DinocoClient<MySqlAdapter>) -> DinocoResult<()> {
    client
        .primary()
        .execute("CREATE TABLE `articles` (`id` VARCHAR(255) PRIMARY KEY, `title` VARCHAR(255) NOT NULL)", &[])
        .await?;
    client
        .primary()
        .execute("CREATE TABLE `labels` (`id` VARCHAR(255) PRIMARY KEY, `name` VARCHAR(255) NOT NULL)", &[])
        .await?;
    client
        .primary()
        .execute(
            "CREATE TABLE `_ArticleLabels` (`article_id` VARCHAR(255) NOT NULL, `label_id` VARCHAR(255) NOT NULL, PRIMARY KEY (`article_id`, `label_id`))",
            &[],
        )
        .await
}

async fn drop_auto_article_tables_postgres(client: &DinocoClient<PostgresAdapter>) -> DinocoResult<()> {
    client.primary().execute(r#"DROP TABLE IF EXISTS "auto_article_labels""#, &[]).await?;
    client.primary().execute(r#"DROP TABLE IF EXISTS "auto_labels""#, &[]).await?;
    client.primary().execute(r#"DROP TABLE IF EXISTS "auto_articles""#, &[]).await
}

async fn create_auto_article_tables_postgres(client: &DinocoClient<PostgresAdapter>) -> DinocoResult<()> {
    client
        .primary()
        .execute(
            r#"CREATE TABLE "auto_articles" (
                "id" BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
                "title" TEXT NOT NULL
            )"#,
            &[],
        )
        .await?;
    client
        .primary()
        .execute(
            r#"CREATE TABLE "auto_labels" (
                "id" BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
                "name" TEXT NOT NULL
            )"#,
            &[],
        )
        .await?;
    client
        .primary()
        .execute(
            r#"CREATE TABLE "auto_article_labels" (
                "article_id" BIGINT NOT NULL,
                "label_id" BIGINT NOT NULL,
                PRIMARY KEY ("article_id", "label_id")
            )"#,
            &[],
        )
        .await
}

async fn drop_auto_article_tables_mysql(client: &DinocoClient<MySqlAdapter>) -> DinocoResult<()> {
    client.primary().execute("DROP TABLE IF EXISTS `auto_article_labels`", &[]).await?;
    client.primary().execute("DROP TABLE IF EXISTS `auto_labels`", &[]).await?;
    client.primary().execute("DROP TABLE IF EXISTS `auto_articles`", &[]).await
}

async fn create_auto_article_tables_mysql(client: &DinocoClient<MySqlAdapter>) -> DinocoResult<()> {
    client
        .primary()
        .execute(
            "CREATE TABLE `auto_articles` (`id` BIGINT AUTO_INCREMENT PRIMARY KEY, `title` VARCHAR(255) NOT NULL)",
            &[],
        )
        .await?;
    client
        .primary()
        .execute(
            "CREATE TABLE `auto_labels` (`id` BIGINT AUTO_INCREMENT PRIMARY KEY, `name` VARCHAR(255) NOT NULL)",
            &[],
        )
        .await?;
    client
        .primary()
        .execute(
            "CREATE TABLE `auto_article_labels` (`article_id` BIGINT NOT NULL, `label_id` BIGINT NOT NULL, PRIMARY KEY (`article_id`, `label_id`))",
            &[],
        )
        .await
}

async fn drop_count_tables_sqlite(client: &DinocoClient<SqliteAdapter>) -> DinocoResult<()> {
    client.primary().execute(&format!(r#"DROP TABLE IF EXISTS "{COMMENTS_TABLE}""#), &[]).await?;
    client.primary().execute(&format!(r#"DROP TABLE IF EXISTS "{POSTS_TABLE}""#), &[]).await?;
    client.primary().execute(&format!(r#"DROP TABLE IF EXISTS "{USERS_TABLE}""#), &[]).await
}

async fn create_count_tables_sqlite(client: &DinocoClient<SqliteAdapter>) -> DinocoResult<()> {
    client.primary().execute(r#"CREATE TABLE "users" ("id" INTEGER PRIMARY KEY, "name" TEXT NOT NULL)"#, &[]).await?;
    client
        .primary()
        .execute(
            r#"CREATE TABLE "posts" (
                "id" INTEGER PRIMARY KEY,
                "title" TEXT NOT NULL,
                "published" BOOLEAN NOT NULL,
                "authorId" INTEGER NOT NULL
            )"#,
            &[],
        )
        .await?;
    client
        .primary()
        .execute(
            r#"CREATE TABLE "comments" (
                "id" INTEGER PRIMARY KEY,
                "text" TEXT NOT NULL,
                "flagged" BOOLEAN NOT NULL,
                "postId" INTEGER NOT NULL
            )"#,
            &[],
        )
        .await
}

async fn drop_count_tables_postgres(client: &DinocoClient<PostgresAdapter>) -> DinocoResult<()> {
    client.primary().execute(&format!(r#"DROP TABLE IF EXISTS "{COMMENTS_TABLE}""#), &[]).await?;
    client.primary().execute(&format!(r#"DROP TABLE IF EXISTS "{POSTS_TABLE}""#), &[]).await?;
    client.primary().execute(&format!(r#"DROP TABLE IF EXISTS "{USERS_TABLE}""#), &[]).await
}

async fn create_count_tables_postgres(client: &DinocoClient<PostgresAdapter>) -> DinocoResult<()> {
    client.primary().execute(r#"CREATE TABLE "users" ("id" BIGINT PRIMARY KEY, "name" TEXT NOT NULL)"#, &[]).await?;
    client
        .primary()
        .execute(
            r#"CREATE TABLE "posts" (
                "id" BIGINT PRIMARY KEY,
                "title" TEXT NOT NULL,
                "published" BOOLEAN NOT NULL,
                "authorId" BIGINT NOT NULL
            )"#,
            &[],
        )
        .await?;
    client
        .primary()
        .execute(
            r#"CREATE TABLE "comments" (
                "id" BIGINT PRIMARY KEY,
                "text" TEXT NOT NULL,
                "flagged" BOOLEAN NOT NULL,
                "postId" BIGINT NOT NULL
            )"#,
            &[],
        )
        .await
}

async fn drop_count_tables_mysql(client: &DinocoClient<MySqlAdapter>) -> DinocoResult<()> {
    client.primary().execute(&format!("DROP TABLE IF EXISTS `{COMMENTS_TABLE}`"), &[]).await?;
    client.primary().execute(&format!("DROP TABLE IF EXISTS `{POSTS_TABLE}`"), &[]).await?;
    client.primary().execute(&format!("DROP TABLE IF EXISTS `{USERS_TABLE}`"), &[]).await
}

async fn create_count_tables_mysql(client: &DinocoClient<MySqlAdapter>) -> DinocoResult<()> {
    client
        .primary()
        .execute("CREATE TABLE `users` (`id` BIGINT PRIMARY KEY, `name` VARCHAR(255) NOT NULL)", &[])
        .await?;
    client
        .primary()
        .execute(
            "CREATE TABLE `posts` (`id` BIGINT PRIMARY KEY, `title` VARCHAR(255) NOT NULL, `published` BOOLEAN NOT NULL, `authorId` BIGINT NOT NULL)",
            &[],
        )
        .await?;
    client
        .primary()
        .execute(
            "CREATE TABLE `comments` (`id` BIGINT PRIMARY KEY, `text` VARCHAR(255) NOT NULL, `flagged` BOOLEAN NOT NULL, `postId` BIGINT NOT NULL)",
            &[],
        )
        .await
}

async fn exercise_relation_insert_flow<A>(client: &DinocoClient<A>) -> DinocoResult<()>
where
    A: DinocoAdapter,
{
    let created_team = insert_into::<Team>()
        .values(Team { id: "team-1".to_string(), name: "Dinoco".to_string() })
        .with_relation(Member { id: "member-1".to_string(), name: "Matheus".to_string(), teamId: String::new() })
        .returning::<Team>()
        .execute(client)
        .await?;

    let created_many = insert_many::<Team>()
        .values(vec![
            Team { id: "team-2".to_string(), name: "Platform".to_string() },
            Team { id: "team-3".to_string(), name: "Compiler".to_string() },
        ])
        .with_relation(vec![
            Member { id: "member-2".to_string(), name: "Ana".to_string(), teamId: String::new() },
            Member { id: "member-3".to_string(), name: "Caio".to_string(), teamId: String::new() },
        ])
        .returning::<Team>()
        .execute(client)
        .await?;

    assert_eq!(created_team.id, "team-1");
    assert_eq!(created_team.name, "Dinoco");
    assert_eq!(created_many.iter().map(|item| item.id.as_str()).collect::<Vec<_>>(), vec!["team-2", "team-3"]);

    let members = find_many::<Member>().order_by(|x| x.id.asc()).execute(client).await?;

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

async fn exercise_relation_insert_flow_autoincrement<A>(client: &DinocoClient<A>) -> DinocoResult<()>
where
    A: DinocoAdapter,
{
    let created_team = insert_into::<AutoTeam>()
        .values(AutoTeam { id: 0, name: "Dinoco".to_string() })
        .with_relation(AutoMember { id: 0, name: "Matheus".to_string(), teamId: 0 })
        .returning::<AutoTeam>()
        .execute(client)
        .await?;

    let created_many = insert_many::<AutoTeam>()
        .values(vec![
            AutoTeam { id: 0, name: "Platform".to_string() },
            AutoTeam { id: 0, name: "Compiler".to_string() },
        ])
        .with_relation(vec![
            AutoMember { id: 0, name: "Ana".to_string(), teamId: 0 },
            AutoMember { id: 0, name: "Caio".to_string(), teamId: 0 },
        ])
        .returning::<AutoTeam>()
        .execute(client)
        .await?;

    assert_eq!(created_team.id, 1);
    assert_eq!(created_many.iter().map(|item| item.id).collect::<Vec<_>>(), vec![2, 3]);

    let members = find_many::<AutoMember>().order_by(|x| x.id.asc()).execute(client).await?;

    assert_eq!(members.iter().map(|item| item.teamId).collect::<Vec<_>>(), vec![1, 2, 3]);

    Ok(())
}

async fn exercise_many_to_many_flow<A>(client: &DinocoClient<A>) -> DinocoResult<()>
where
    A: DinocoAdapter,
{
    let created_article = insert_into::<Article>()
        .values(Article { id: "article-1".to_string(), title: "Dinoco Connect".to_string() })
        .with_relation(Label { id: "label-1".to_string(), name: "orm".to_string() })
        .returning::<Article>()
        .execute(client)
        .await?;

    let created_many = insert_many::<Article>()
        .values(vec![
            Article { id: "article-2".to_string(), title: "Dinoco Insert Many".to_string() },
            Article { id: "article-3".to_string(), title: "Dinoco Disconnect".to_string() },
        ])
        .with_relation(vec![
            Label { id: "label-2".to_string(), name: "rust".to_string() },
            Label { id: "label-3".to_string(), name: "mysql".to_string() },
        ])
        .returning::<Article>()
        .execute(client)
        .await?;

    assert_eq!(created_article.id, "article-1");
    assert_eq!(created_many.iter().map(|item| item.id.as_str()).collect::<Vec<_>>(), vec!["article-2", "article-3"]);

    update::<Article>().cond(|x| x.id.eq("article-1")).connect(|x| x.labels().id.eq("label-2")).execute(client).await?;

    update::<Article>()
        .cond(|x| x.id.eq("article-1"))
        .disconnect(|x| x.labels().id.eq("label-1"))
        .execute(client)
        .await?;

    let rows = find_many::<ArticleLabel>().order_by(|x| x.article_id.asc()).execute(client).await?;

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

async fn exercise_many_to_many_flow_autoincrement<A>(client: &DinocoClient<A>) -> DinocoResult<()>
where
    A: DinocoAdapter,
{
    let created_article = insert_into::<AutoArticle>()
        .values(AutoArticle { id: 0, title: "Dinoco Connect".to_string() })
        .with_relation(AutoLabel { id: 0, name: "orm".to_string() })
        .returning::<AutoArticle>()
        .execute(client)
        .await?;

    let created_many = insert_many::<AutoArticle>()
        .values(vec![
            AutoArticle { id: 0, title: "Dinoco Insert Many".to_string() },
            AutoArticle { id: 0, title: "Dinoco Disconnect".to_string() },
        ])
        .with_relation(vec![
            AutoLabel { id: 0, name: "rust".to_string() },
            AutoLabel { id: 0, name: "mysql".to_string() },
        ])
        .returning::<AutoArticle>()
        .execute(client)
        .await?;

    assert_eq!(created_article.id, 1);
    assert_eq!(created_many.iter().map(|item| item.id).collect::<Vec<_>>(), vec![2, 3]);

    let rows = find_many::<AutoArticleLabel>().order_by(|x| x.article_id.asc()).execute(client).await?;

    assert_eq!(rows.iter().map(|row| (row.article_id, row.label_id)).collect::<Vec<_>>(), vec![(1, 1), (2, 2), (3, 3)]);

    Ok(())
}

async fn exercise_insert_connection_flow<A>(client: &DinocoClient<A>) -> DinocoResult<()>
where
    A: DinocoAdapter,
{
    insert_many::<Member>()
        .values(vec![
            Member { id: "member-10".to_string(), name: "Julia".to_string(), teamId: "legacy".to_string() },
            Member { id: "member-11".to_string(), name: "Rafa".to_string(), teamId: "legacy".to_string() },
            Member { id: "member-12".to_string(), name: "Bia".to_string(), teamId: "legacy".to_string() },
        ])
        .execute(client)
        .await?;

    insert_many::<Label>()
        .values(vec![
            Label { id: "label-10".to_string(), name: "backend".to_string() },
            Label { id: "label-11".to_string(), name: "orm".to_string() },
            Label { id: "label-12".to_string(), name: "rust".to_string() },
        ])
        .execute(client)
        .await?;

    let connected_team = insert_into::<Team>()
        .values(Team { id: "team-10".to_string(), name: "Infra".to_string() })
        .with_connection(Member {
            id: "member-10".to_string(),
            name: "Julia".to_string(),
            teamId: "legacy".to_string(),
        })
        .returning::<Team>()
        .execute(client)
        .await?;

    let connected_many = insert_many::<Team>()
        .values(vec![
            Team { id: "team-11".to_string(), name: "Data".to_string() },
            Team { id: "team-12".to_string(), name: "DX".to_string() },
        ])
        .with_connection(vec![
            Member { id: "member-11".to_string(), name: "Rafa".to_string(), teamId: "legacy".to_string() },
            Member { id: "member-12".to_string(), name: "Bia".to_string(), teamId: "legacy".to_string() },
        ])
        .returning::<Team>()
        .execute(client)
        .await?;

    assert_eq!(connected_team.id, "team-10");
    assert_eq!(connected_many.iter().map(|item| item.id.as_str()).collect::<Vec<_>>(), vec!["team-11", "team-12"]);

    let members = find_many::<Member>().order_by(|x| x.id.asc()).execute(client).await?;

    assert_eq!(
        members.iter().map(|item| (&item.id, &item.teamId)).collect::<Vec<_>>(),
        vec![
            (&"member-10".to_string(), &"team-10".to_string()),
            (&"member-11".to_string(), &"team-11".to_string()),
            (&"member-12".to_string(), &"team-12".to_string()),
        ]
    );

    let connected_article = insert_into::<Article>()
        .values(Article { id: "article-10".to_string(), title: "Connect Existing".to_string() })
        .with_connection(Label { id: "label-10".to_string(), name: "backend".to_string() })
        .returning::<Article>()
        .execute(client)
        .await?;

    let connected_articles = insert_many::<Article>()
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
        .returning::<Article>()
        .execute(client)
        .await?;

    assert_eq!(connected_article.id, "article-10");
    assert_eq!(
        connected_articles.iter().map(|item| item.id.as_str()).collect::<Vec<_>>(),
        vec!["article-11", "article-12"]
    );

    let rows = find_many::<ArticleLabel>().order_by(|x| x.article_id.asc()).execute(client).await?;

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

async fn exercise_insert_connection_payload_flow<A>(client: &DinocoClient<A>) -> DinocoResult<()>
where
    A: DinocoAdapter,
{
    insert_many::<Label>()
        .values(vec![
            Label { id: "label-40".to_string(), name: "orm".to_string() },
            Label { id: "label-41".to_string(), name: "rust".to_string() },
            Label { id: "label-42".to_string(), name: "backend".to_string() },
        ])
        .execute(client)
        .await?;

    let created_article = insert_into::<Article>()
        .values(ArticleWithConnectionPayload {
            id: "article-40".to_string(),
            title: "Single Connect Payload".to_string(),
            labels: vec![ArticleConnection::Label("label-40".to_string())],
        })
        .returning::<Article>()
        .execute(client)
        .await?;

    let created_many = insert_many::<Article>()
        .values(vec![
            ArticleWithConnectionPayload {
                id: "article-41".to_string(),
                title: "Connect Multiple".to_string(),
                labels: vec![
                    ArticleConnection::Label("label-40".to_string()),
                    ArticleConnection::Label("label-41".to_string()),
                ],
            },
            ArticleWithConnectionPayload {
                id: "article-42".to_string(),
                title: "Connect Batch".to_string(),
                labels: vec![ArticleConnection::Label("label-42".to_string())],
            },
        ])
        .returning::<Article>()
        .execute(client)
        .await?;

    assert_eq!(created_article.id, "article-40");
    assert_eq!(created_many.iter().map(|item| item.id.as_str()).collect::<Vec<_>>(), vec!["article-41", "article-42"]);

    let rows = find_many::<ArticleLabel>().order_by(|x| x.article_id.asc()).execute(client).await?;

    assert_eq!(
        rows.iter().map(|row| (&row.article_id, &row.label_id)).collect::<Vec<_>>(),
        vec![
            (&"article-40".to_string(), &"label-40".to_string()),
            (&"article-41".to_string(), &"label-40".to_string()),
            (&"article-41".to_string(), &"label-41".to_string()),
            (&"article-42".to_string(), &"label-42".to_string()),
        ]
    );

    Ok(())
}

async fn exercise_recursive_insert_payload_flow<A>(client: &DinocoClient<A>) -> DinocoResult<()>
where
    A: DinocoAdapter,
{
    insert_into::<User>()
        .values(UserInsertItem {
            id: 10,
            name: "Alice".to_string(),
            posts: vec![
                PostInsertItem {
                    id: 100,
                    title: "Nested One".to_string(),
                    published: true,
                    authorId: 0,
                    comments: vec![
                        CommentInsertItem { id: 1000, text: "c1".to_string(), flagged: false, postId: 0 },
                        CommentInsertItem { id: 1001, text: "c2".to_string(), flagged: true, postId: 0 },
                    ],
                },
                PostInsertItem {
                    id: 101,
                    title: "Nested Two".to_string(),
                    published: false,
                    authorId: 0,
                    comments: vec![CommentInsertItem { id: 1002, text: "c3".to_string(), flagged: false, postId: 0 }],
                },
            ],
        })
        .execute(client)
        .await?;

    insert_many::<User>()
        .values(vec![UserInsertItem {
            id: 11,
            name: "Bruno".to_string(),
            posts: vec![PostInsertItem {
                id: 102,
                title: "Nested Batch".to_string(),
                published: true,
                authorId: 0,
                comments: vec![CommentInsertItem { id: 1003, text: "c4".to_string(), flagged: false, postId: 0 }],
            }],
        }])
        .execute(client)
        .await?;

    let posts = find_many::<Post>().order_by(|x| x.id.asc()).execute(client).await?;
    let comments = find_many::<Comment>().order_by(|x| x.id.asc()).execute(client).await?;

    assert_eq!(
        posts.iter().map(|item| (item.id, item.authorId)).collect::<Vec<_>>(),
        vec![(100, 10), (101, 10), (102, 11)]
    );
    assert_eq!(
        comments.iter().map(|item| (item.id, item.postId)).collect::<Vec<_>>(),
        vec![(1000, 100), (1001, 100), (1002, 101), (1003, 102)]
    );

    Ok(())
}

async fn exercise_relation_count_flow<A>(client: &DinocoClient<A>) -> DinocoResult<()>
where
    A: DinocoAdapter,
{
    insert_many::<User>()
        .values(vec![User { id: 1, name: "Alice".to_string() }, User { id: 2, name: "Bruno".to_string() }])
        .execute(client)
        .await?;

    insert_many::<Post>()
        .values(vec![
            Post { id: 1, title: "A1".to_string(), published: true, authorId: 1 },
            Post { id: 2, title: "A2".to_string(), published: false, authorId: 1 },
            Post { id: 3, title: "A3".to_string(), published: true, authorId: 1 },
            Post { id: 4, title: "B1".to_string(), published: true, authorId: 2 },
        ])
        .execute(client)
        .await?;

    insert_many::<Comment>()
        .values(vec![
            Comment { id: 1, text: "c1".to_string(), flagged: false, postId: 1 },
            Comment { id: 2, text: "c2".to_string(), flagged: false, postId: 1 },
            Comment { id: 3, text: "c3".to_string(), flagged: true, postId: 1 },
            Comment { id: 4, text: "c4".to_string(), flagged: false, postId: 2 },
            Comment { id: 5, text: "c5".to_string(), flagged: false, postId: 3 },
            Comment { id: 6, text: "c6".to_string(), flagged: false, postId: 4 },
            Comment { id: 7, text: "c7".to_string(), flagged: true, postId: 4 },
        ])
        .execute(client)
        .await?;

    let users = find_many::<User>()
        .select::<UserListItem>()
        .order_by(|x| x.id.asc())
        .count(|x| x.posts().cond(|post| post.published.eq(true)))
        .includes(|x| {
            x.posts()
                .select::<PostListItem>()
                .order_by(|post| post.id.asc())
                .count(|post| post.comments().cond(|comment| comment.flagged.eq(false)))
                .includes(|post| {
                    post.comments()
                        .cond(|comment| comment.flagged.eq(false))
                        .order_by(|comment| comment.id.asc())
                        .select::<CommentListItem>()
                })
        })
        .execute(client)
        .await?;

    assert_eq!(users.len(), 2);
    assert_eq!(users[0].posts_count, 2);
    assert_eq!(users[1].posts_count, 1);
    assert_eq!(users[0].posts[0].comments_count, 2);
    assert_eq!(users[0].posts[1].comments_count, 1);
    assert_eq!(users[1].posts[0].comments_count, 1);

    let first_user = find_first::<User>()
        .select::<UserListItem>()
        .cond(|x| x.id.eq(1_i64))
        .count(|x| x.posts().cond(|post| post.published.eq(true)))
        .includes(|x| {
            x.posts()
                .select::<PostListItem>()
                .order_by(|post| post.id.asc())
                .count(|post| post.comments().cond(|comment| comment.flagged.eq(false)))
        })
        .execute(client)
        .await?
        .expect("first user should exist");

    assert_eq!(first_user.id, 1);
    assert_eq!(first_user.posts_count, 2);
    assert_eq!(first_user.posts[0].comments_count, 2);

    Ok(())
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

impl Projection<AutoTeam> for AutoTeam {
    fn columns() -> &'static [&'static str] {
        &["id", "name"]
    }
}

impl Projection<AutoMember> for AutoMember {
    fn columns() -> &'static [&'static str] {
        &["id", "name", "teamId"]
    }
}

impl Default for AutoTeamWhere {
    fn default() -> Self {
        Self { id: ScalarField::new("id"), name: ScalarField::new("name") }
    }
}

impl Default for AutoMemberWhere {
    fn default() -> Self {
        Self { id: ScalarField::new("id"), name: ScalarField::new("name"), teamId: ScalarField::new("teamId") }
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

impl Projection<AutoArticle> for AutoArticle {
    fn columns() -> &'static [&'static str] {
        &["id", "title"]
    }
}

impl Projection<AutoLabel> for AutoLabel {
    fn columns() -> &'static [&'static str] {
        &["id", "name"]
    }
}

impl Projection<AutoArticleLabel> for AutoArticleLabel {
    fn columns() -> &'static [&'static str] {
        &["article_id", "label_id"]
    }
}

impl Default for AutoArticleWhere {
    fn default() -> Self {
        Self { id: ScalarField::new("id"), title: ScalarField::new("title") }
    }
}

impl Default for AutoLabelWhere {
    fn default() -> Self {
        Self { id: ScalarField::new("id"), name: ScalarField::new("name") }
    }
}

impl Default for AutoArticleLabelWhere {
    fn default() -> Self {
        Self { article_id: ScalarField::new("article_id"), label_id: ScalarField::new("label_id") }
    }
}

impl Projection<User> for User {
    fn columns() -> &'static [&'static str] {
        &["id", "name"]
    }
}

impl Projection<Post> for Post {
    fn columns() -> &'static [&'static str] {
        &["id", "title", "published", "authorId"]
    }
}

impl Projection<Comment> for Comment {
    fn columns() -> &'static [&'static str] {
        &["id", "text", "flagged", "postId"]
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

impl InsertModel for AutoTeam {
    fn insert_columns() -> &'static [&'static str] {
        &["name"]
    }

    fn into_insert_row(self) -> Vec<DinocoValue> {
        vec![self.name.into()]
    }

    fn insert_identity_conditions(&self) -> Vec<dinoco_engine::Expression> {
        vec![dinoco_engine::Expression::Column("id".to_string()).eq(self.id)]
    }

    fn auto_increment_primary_key_column() -> Option<&'static str> {
        Some("id")
    }
}

impl InsertModel for AutoMember {
    fn insert_columns() -> &'static [&'static str] {
        &["name", "teamId"]
    }

    fn into_insert_row(self) -> Vec<DinocoValue> {
        vec![self.name.into(), self.teamId.into()]
    }

    fn insert_identity_conditions(&self) -> Vec<dinoco_engine::Expression> {
        vec![dinoco_engine::Expression::Column("id".to_string()).eq(self.id)]
    }

    fn auto_increment_primary_key_column() -> Option<&'static str> {
        Some("id")
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

impl InsertModel for AutoArticle {
    fn insert_columns() -> &'static [&'static str] {
        &["title"]
    }

    fn into_insert_row(self) -> Vec<DinocoValue> {
        vec![self.title.into()]
    }

    fn insert_identity_conditions(&self) -> Vec<dinoco_engine::Expression> {
        vec![dinoco_engine::Expression::Column("id".to_string()).eq(self.id)]
    }

    fn auto_increment_primary_key_column() -> Option<&'static str> {
        Some("id")
    }
}

impl InsertModel for AutoLabel {
    fn insert_columns() -> &'static [&'static str] {
        &["name"]
    }

    fn into_insert_row(self) -> Vec<DinocoValue> {
        vec![self.name.into()]
    }

    fn insert_identity_conditions(&self) -> Vec<dinoco_engine::Expression> {
        vec![dinoco_engine::Expression::Column("id".to_string()).eq(self.id)]
    }

    fn auto_increment_primary_key_column() -> Option<&'static str> {
        Some("id")
    }
}

impl InsertModel for AutoArticleLabel {
    fn insert_columns() -> &'static [&'static str] {
        &["article_id", "label_id"]
    }

    fn into_insert_row(self) -> Vec<DinocoValue> {
        vec![self.article_id.into(), self.label_id.into()]
    }

    fn insert_identity_conditions(&self) -> Vec<dinoco_engine::Expression> {
        vec![
            dinoco_engine::Expression::Column("article_id".to_string()).eq(self.article_id),
            dinoco_engine::Expression::Column("label_id".to_string()).eq(self.label_id),
        ]
    }
}

impl InsertModel for User {
    fn insert_columns() -> &'static [&'static str] {
        &["id", "name"]
    }

    fn into_insert_row(self) -> Vec<DinocoValue> {
        vec![self.id.into(), self.name.into()]
    }

    fn insert_identity_conditions(&self) -> Vec<dinoco_engine::Expression> {
        vec![dinoco_engine::Expression::Column("id".to_string()).eq(self.id)]
    }
}

impl InsertModel for Post {
    fn insert_columns() -> &'static [&'static str] {
        &["id", "title", "published", "authorId"]
    }

    fn into_insert_row(self) -> Vec<DinocoValue> {
        vec![self.id.into(), self.title.into(), self.published.into(), self.authorId.into()]
    }

    fn insert_identity_conditions(&self) -> Vec<dinoco_engine::Expression> {
        vec![dinoco_engine::Expression::Column("id".to_string()).eq(self.id)]
    }
}

impl InsertModel for Comment {
    fn insert_columns() -> &'static [&'static str] {
        &["id", "text", "flagged", "postId"]
    }

    fn into_insert_row(self) -> Vec<DinocoValue> {
        vec![self.id.into(), self.text.into(), self.flagged.into(), self.postId.into()]
    }

    fn insert_identity_conditions(&self) -> Vec<dinoco_engine::Expression> {
        vec![dinoco_engine::Expression::Column("id".to_string()).eq(self.id)]
    }
}

impl InsertRelation<Member> for Team {
    fn bind_relation(&self, item: &mut Member) {
        item.teamId = self.id.clone();
    }
}

impl InsertRelation<AutoMember> for AutoTeam {
    fn bind_relation(&self, item: &mut AutoMember) {
        item.teamId = self.id;
    }
}

impl InsertRelation<Post> for User {
    fn bind_relation(&self, item: &mut Post) {
        item.authorId = self.id;
    }
}

impl InsertRelation<Comment> for Post {
    fn bind_relation(&self, item: &mut Comment) {
        item.postId = self.id;
    }
}

impl InsertConnection<Member> for Team {
    fn connection_updates(&self, item: &Member) -> Vec<dinoco::ConnectionUpdatePlan> {
        vec![dinoco::ConnectionUpdatePlan {
            table_name: MEMBERS_TABLE,
            columns: &["name", "teamId"],
            row: vec![item.name.clone().into(), self.id.clone().into()],
            conditions: vec![dinoco_engine::Expression::Column("id".to_string()).eq(item.id.clone())],
        }]
    }
}

impl InsertRelation<Label> for Article {
    fn relation_links(&self, item: &Label) -> Vec<RelationLinkPlan> {
        vec![RelationLinkPlan {
            table_name: ARTICLE_LABELS_TABLE,
            columns: &["article_id", "label_id"],
            row: vec![self.id.clone().into(), item.id.clone().into()],
        }]
    }
}

impl InsertRelation<AutoLabel> for AutoArticle {
    fn relation_links(&self, item: &AutoLabel) -> Vec<RelationLinkPlan> {
        vec![RelationLinkPlan {
            table_name: "auto_article_labels",
            columns: &["article_id", "label_id"],
            row: vec![self.id.into(), item.id.into()],
        }]
    }
}

impl InsertConnection<Label> for Article {}
impl InsertConnection<AutoMember> for AutoTeam {}
impl InsertConnection<AutoLabel> for AutoArticle {}

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

impl Model for Team {
    type Include = TeamInclude;
    type Where = TeamWhere;

    fn table_name() -> &'static str {
        TEAMS_TABLE
    }
}

impl Model for Member {
    type Include = MemberInclude;
    type Where = MemberWhere;

    fn table_name() -> &'static str {
        MEMBERS_TABLE
    }
}

impl Model for AutoTeam {
    type Include = AutoTeamInclude;
    type Where = AutoTeamWhere;

    fn table_name() -> &'static str {
        "auto_teams"
    }
}

impl Model for AutoMember {
    type Include = AutoMemberInclude;
    type Where = AutoMemberWhere;

    fn table_name() -> &'static str {
        "auto_members"
    }
}

impl Model for Article {
    type Include = ArticleInclude;
    type Where = ArticleWhere;

    fn table_name() -> &'static str {
        ARTICLES_TABLE
    }
}

impl Model for Label {
    type Include = LabelInclude;
    type Where = LabelWhere;

    fn table_name() -> &'static str {
        LABELS_TABLE
    }
}

impl Model for ArticleLabel {
    type Include = ArticleLabelInclude;
    type Where = ArticleLabelWhere;

    fn table_name() -> &'static str {
        ARTICLE_LABELS_TABLE
    }
}

impl Model for AutoArticle {
    type Include = AutoArticleInclude;
    type Where = AutoArticleWhere;

    fn table_name() -> &'static str {
        "auto_articles"
    }
}

impl Model for AutoLabel {
    type Include = AutoLabelInclude;
    type Where = AutoLabelWhere;

    fn table_name() -> &'static str {
        "auto_labels"
    }
}

impl Model for AutoArticleLabel {
    type Include = AutoArticleLabelInclude;
    type Where = AutoArticleLabelWhere;

    fn table_name() -> &'static str {
        "auto_article_labels"
    }
}

impl Model for User {
    type Include = UserInclude;
    type Where = UserWhere;

    fn table_name() -> &'static str {
        USERS_TABLE
    }
}

impl Model for Post {
    type Include = PostInclude;
    type Where = PostWhere;

    fn table_name() -> &'static str {
        POSTS_TABLE
    }
}

impl Model for Comment {
    type Include = CommentInclude;
    type Where = CommentWhere;

    fn table_name() -> &'static str {
        COMMENTS_TABLE
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

impl Default for ArticleWhere {
    fn default() -> Self {
        Self { id: ScalarField::new("id"), title: ScalarField::new("title") }
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

impl Default for LabelWhere {
    fn default() -> Self {
        Self { id: ScalarField::new("id"), name: ScalarField::new("name") }
    }
}

impl Default for LabelInclude {
    fn default() -> Self {
        Self {}
    }
}

impl Default for ArticleLabelWhere {
    fn default() -> Self {
        Self { article_id: ScalarField::new("article_id"), label_id: ScalarField::new("label_id") }
    }
}

impl Default for ArticleLabelInclude {
    fn default() -> Self {
        Self {}
    }
}

impl Default for UserWhere {
    fn default() -> Self {
        Self { id: ScalarField::new("id"), name: ScalarField::new("name") }
    }
}

impl Default for UserInclude {
    fn default() -> Self {
        Self {}
    }
}

impl Default for PostWhere {
    fn default() -> Self {
        Self {
            id: ScalarField::new("id"),
            title: ScalarField::new("title"),
            published: ScalarField::new("published"),
            authorId: ScalarField::new("authorId"),
        }
    }
}

impl Default for PostInclude {
    fn default() -> Self {
        Self {}
    }
}

impl Default for CommentWhere {
    fn default() -> Self {
        Self {
            id: ScalarField::new("id"),
            text: ScalarField::new("text"),
            flagged: ScalarField::new("flagged"),
            postId: ScalarField::new("postId"),
        }
    }
}

impl Default for CommentInclude {
    fn default() -> Self {
        Self {}
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

impl ArticleRelations {
    fn labels(&self) -> RelationMutationWhere<LabelRelationWhere> {
        LabelRelationWhere::new("labels")
    }
}

impl RelationMutationModel for Article {
    type Relations = ArticleRelations;

    fn relation_write_plan(target: dinoco::RelationMutationTarget) -> Option<RelationWritePlan> {
        match target.relation_name {
            "labels" => Some(RelationWritePlan {
                join_table_name: ARTICLE_LABELS_TABLE,
                source_table_name: ARTICLES_TABLE,
                source_key_column: "id",
                source_join_column: "article_id",
                target_table_name: LABELS_TABLE,
                target_key_column: "id",
                target_join_column: "label_id",
                target_expression: target.expression,
            }),
            _ => None,
        }
    }
}

impl UserInclude {
    fn posts(&self) -> RelationField<Post> {
        RelationField::new("posts")
    }
}

impl PostInclude {
    fn comments(&self) -> RelationField<Comment> {
        RelationField::new("comments")
    }
}

impl User {
    pub fn __dinoco_load_posts<'a, P, C, A>(
        item_keys: Vec<Option<i64>>,
        include: &'a dinoco::IncludeNode,
        client: &'a DinocoClient<A>,
        read_mode: dinoco::ReadMode,
        relation_field: impl Fn(&mut P) -> &mut Vec<C> + Copy + 'a,
    ) -> IncludeLoaderFuture<'a, P>
    where
        A: DinocoAdapter,
        C: Projection<Post>,
    {
        Box::pin(async move {
            struct PostRow<C> {
                item: C,
                relation_key: i64,
            }

            impl<C> DinocoRow for PostRow<C>
            where
                C: Projection<Post>,
            {
                fn from_row<R: DinocoGenericRow>(row: &R) -> DinocoResult<Self> {
                    Ok(Self { item: C::from_row(row)?, relation_key: row.get(C::columns().len())? })
                }
            }

            let keys = item_keys.iter().flatten().cloned().collect::<Vec<_>>();

            if keys.is_empty() {
                return Ok(Box::new(|_: &mut [P]| {}) as dinoco::IncludeApplier<'a, P>);
            }

            let adapter = client.read_adapter(false);
            let mut statement = include
                .statement
                .clone()
                .unwrap_or_else(|| dinoco_engine::SelectStatement::new().from(POSTS_TABLE).select(C::columns()));
            let mut select_columns = statement.select.clone();

            if select_columns.is_empty() {
                select_columns = C::columns().iter().map(|column| column.to_string()).collect::<Vec<_>>();
            }

            select_columns.push("authorId".to_string());
            statement.select = select_columns;
            statement.conditions.push(
                dinoco_engine::Expression::Column("authorId".to_string())
                    .in_values(keys.iter().cloned().map(IntoDinocoValue::into_dinoco_value).collect()),
            );

            let (sql, params) = dinoco_engine::QueryBuilder::build_select(adapter.dialect(), &statement);
            let rows = adapter.query_as::<PostRow<C>>(&sql, &params).await?;
            let relation_keys = rows.iter().map(|row| row.relation_key).collect::<Vec<_>>();
            let mut children = rows.into_iter().map(|row| row.item).collect::<Vec<_>>();

            C::load_includes(&mut children, &include.includes, client, read_mode).await?;
            C::load_counts(&mut children, &include.counts, client, read_mode).await?;

            let mut grouped: HashMap<i64, Vec<C>> = HashMap::new();

            for (relation_key, child) in relation_keys.into_iter().zip(children.into_iter()) {
                grouped.entry(relation_key).or_default().push(child);
            }

            Ok(Box::new(move |items: &mut [P]| {
                for (item, key) in items.iter_mut().zip(item_keys.into_iter()) {
                    *relation_field(item) = key.and_then(|key| grouped.remove(&key)).unwrap_or_default();
                }
            }) as dinoco::IncludeApplier<'a, P>)
        })
    }

    pub fn __dinoco_count_posts<'a, P, A>(
        item_keys: Vec<Option<i64>>,
        count: &'a dinoco::CountNode,
        client: &'a DinocoClient<A>,
        _read_mode: dinoco::ReadMode,
        relation_field: impl Fn(&mut P) -> &mut usize + Copy + 'a,
    ) -> IncludeLoaderFuture<'a, P>
    where
        A: DinocoAdapter,
    {
        Box::pin(async move {
            #[derive(Debug, Clone, Rowable)]
            struct CountRow {
                authorId: i64,
            }

            let keys = item_keys.iter().flatten().cloned().collect::<Vec<_>>();

            if keys.is_empty() {
                return Ok(Box::new(|_: &mut [P]| {}) as dinoco::IncludeApplier<'a, P>);
            }

            let adapter = client.read_adapter(false);
            let mut statement = count
                .statement
                .clone()
                .unwrap_or_else(|| dinoco_engine::SelectStatement::new().from(POSTS_TABLE).select(&["authorId"]));
            statement.select = vec!["authorId".to_string()];
            statement.conditions.push(
                dinoco_engine::Expression::Column("authorId".to_string())
                    .in_values(keys.iter().cloned().map(IntoDinocoValue::into_dinoco_value).collect()),
            );

            let (sql, params) = if statement.limit.is_some() || statement.skip.is_some() {
                dinoco_engine::QueryBuilder::build_partitioned_select(
                    adapter.dialect(),
                    &statement,
                    "authorId",
                    "__dinoco_row_num",
                )
            } else {
                dinoco_engine::QueryBuilder::build_select(adapter.dialect(), &statement)
            };
            let rows = adapter.query_as::<CountRow>(&sql, &params).await?;
            let mut grouped: HashMap<i64, usize> = HashMap::new();

            for row in rows {
                *grouped.entry(row.authorId).or_default() += 1;
            }

            Ok(Box::new(move |items: &mut [P]| {
                for (item, key) in items.iter_mut().zip(item_keys.into_iter()) {
                    *relation_field(item) = key.and_then(|key| grouped.remove(&key)).unwrap_or(0);
                }
            }) as dinoco::IncludeApplier<'a, P>)
        })
    }
}

impl Post {
    pub fn __dinoco_load_comments<'a, P, C, A>(
        item_keys: Vec<Option<i64>>,
        include: &'a dinoco::IncludeNode,
        client: &'a DinocoClient<A>,
        read_mode: dinoco::ReadMode,
        relation_field: impl Fn(&mut P) -> &mut Vec<C> + Copy + 'a,
    ) -> IncludeLoaderFuture<'a, P>
    where
        A: DinocoAdapter,
        C: Projection<Comment>,
    {
        Box::pin(async move {
            struct CommentRow<C> {
                item: C,
                relation_key: i64,
            }

            impl<C> DinocoRow for CommentRow<C>
            where
                C: Projection<Comment>,
            {
                fn from_row<R: DinocoGenericRow>(row: &R) -> DinocoResult<Self> {
                    Ok(Self { item: C::from_row(row)?, relation_key: row.get(C::columns().len())? })
                }
            }

            let keys = item_keys.iter().flatten().cloned().collect::<Vec<_>>();

            if keys.is_empty() {
                return Ok(Box::new(|_: &mut [P]| {}) as dinoco::IncludeApplier<'a, P>);
            }

            let adapter = client.read_adapter(false);
            let mut statement = include
                .statement
                .clone()
                .unwrap_or_else(|| dinoco_engine::SelectStatement::new().from(COMMENTS_TABLE).select(C::columns()));
            let mut select_columns = statement.select.clone();

            if select_columns.is_empty() {
                select_columns = C::columns().iter().map(|column| column.to_string()).collect::<Vec<_>>();
            }

            select_columns.push("postId".to_string());
            statement.select = select_columns;
            statement.conditions.push(
                dinoco_engine::Expression::Column("postId".to_string())
                    .in_values(keys.iter().cloned().map(IntoDinocoValue::into_dinoco_value).collect()),
            );

            let (sql, params) = dinoco_engine::QueryBuilder::build_select(adapter.dialect(), &statement);
            let rows = adapter.query_as::<CommentRow<C>>(&sql, &params).await?;
            let relation_keys = rows.iter().map(|row| row.relation_key).collect::<Vec<_>>();
            let mut children = rows.into_iter().map(|row| row.item).collect::<Vec<_>>();

            C::load_includes(&mut children, &include.includes, client, read_mode).await?;
            C::load_counts(&mut children, &include.counts, client, read_mode).await?;

            let mut grouped: HashMap<i64, Vec<C>> = HashMap::new();

            for (relation_key, child) in relation_keys.into_iter().zip(children.into_iter()) {
                grouped.entry(relation_key).or_default().push(child);
            }

            Ok(Box::new(move |items: &mut [P]| {
                for (item, key) in items.iter_mut().zip(item_keys.into_iter()) {
                    *relation_field(item) = key.and_then(|key| grouped.remove(&key)).unwrap_or_default();
                }
            }) as dinoco::IncludeApplier<'a, P>)
        })
    }

    pub fn __dinoco_count_comments<'a, P, A>(
        item_keys: Vec<Option<i64>>,
        count: &'a dinoco::CountNode,
        client: &'a DinocoClient<A>,
        _read_mode: dinoco::ReadMode,
        relation_field: impl Fn(&mut P) -> &mut usize + Copy + 'a,
    ) -> IncludeLoaderFuture<'a, P>
    where
        A: DinocoAdapter,
    {
        Box::pin(async move {
            #[derive(Debug, Clone, Rowable)]
            struct CountRow {
                postId: i64,
            }

            let keys = item_keys.iter().flatten().cloned().collect::<Vec<_>>();

            if keys.is_empty() {
                return Ok(Box::new(|_: &mut [P]| {}) as dinoco::IncludeApplier<'a, P>);
            }

            let adapter = client.read_adapter(false);
            let mut statement = count
                .statement
                .clone()
                .unwrap_or_else(|| dinoco_engine::SelectStatement::new().from(COMMENTS_TABLE).select(&["postId"]));
            statement.select = vec!["postId".to_string()];
            statement.conditions.push(
                dinoco_engine::Expression::Column("postId".to_string())
                    .in_values(keys.iter().cloned().map(IntoDinocoValue::into_dinoco_value).collect()),
            );

            let (sql, params) = if statement.limit.is_some() || statement.skip.is_some() {
                dinoco_engine::QueryBuilder::build_partitioned_select(
                    adapter.dialect(),
                    &statement,
                    "postId",
                    "__dinoco_row_num",
                )
            } else {
                dinoco_engine::QueryBuilder::build_select(adapter.dialect(), &statement)
            };
            let rows = adapter.query_as::<CountRow>(&sql, &params).await?;
            let mut grouped: HashMap<i64, usize> = HashMap::new();

            for row in rows {
                *grouped.entry(row.postId).or_default() += 1;
            }

            Ok(Box::new(move |items: &mut [P]| {
                for (item, key) in items.iter_mut().zip(item_keys.into_iter()) {
                    *relation_field(item) = key.and_then(|key| grouped.remove(&key)).unwrap_or(0);
                }
            }) as dinoco::IncludeApplier<'a, P>)
        })
    }
}
