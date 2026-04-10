use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, OnceLock};

use dinoco_codegen::generate_models;
use dinoco_compiler::compile;
use tempfile::TempDir;

struct CurrentDirGuard {
    original: PathBuf,
}

fn current_dir_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();

    LOCK.get_or_init(|| Mutex::new(()))
}

fn lock_current_dir() -> MutexGuard<'static, ()> {
    current_dir_lock().lock().unwrap_or_else(|error| error.into_inner())
}

impl CurrentDirGuard {
    fn change_to(path: &Path) -> Self {
        let original = env::current_dir().expect("current dir should exist");

        env::set_current_dir(path).expect("should change current dir");

        Self { original }
    }
}

impl Drop for CurrentDirGuard {
    fn drop(&mut self) {
        env::set_current_dir(&self.original).expect("should restore current dir");
    }
}

#[test]
fn generate_models_writes_expected_files() {
    let _lock = lock_current_dir();
    let raw = r#"
config {
    database = "sqlite"
    database_url = env("DATABASE_URL")
}

model Post {
    id String @id @default(uuid())
    name String
    content String?
    authorId Integer
}
"#;
    let (_, parsed) = compile(raw).expect("schema should compile");
    let temp_dir = TempDir::new().expect("temp dir should be created");
    let _guard = CurrentDirGuard::change_to(temp_dir.path());

    generate_models(parsed);

    let model_file =
        fs::read_to_string(temp_dir.path().join("dinoco/models/post.rs")).expect("generated post model should exist");
    let dinoco_module =
        fs::read_to_string(temp_dir.path().join("dinoco/mod.rs")).expect("generated dinoco module should exist");

    assert!(model_file.contains("pub struct Post"));
    assert!(
        model_file.contains("#[derive(Debug, Clone, dinoco::serde::Serialize, dinoco::serde::Deserialize, Rowable)]")
    );
    assert!(model_file.contains("#[serde(crate = \"dinoco::serde\")]"));
    assert!(model_file.contains("fn validate_insert(&self) -> dinoco::DinocoResult<()>"));
    assert!(model_file.contains("Field 'Post.name' is required for insert and cannot be empty"));
    assert!(model_file.contains("impl Model for Post"));
    assert!(dinoco_module.contains("create_connection(config: DinocoClientConfig)"));
    assert!(dinoco_module.contains("DinocoClient::<SqliteAdapter>::new(std::env::var(\"DATABASE_URL\")"));
}

#[test]
fn generate_models_prefers_default_derives_when_defaults_match_rust_defaults() {
    let _lock = lock_current_dir();
    let raw = r#"
config {
    database = "sqlite"
    database_url = env("DATABASE_URL")
}

enum Role {
    USER
    ADMIN
}

model User {
    id Integer @id @default(autoincrement())
    name String
    active Boolean
    payload Json
    role Role
}
"#;
    let (_, parsed) = compile(raw).expect("schema should compile");
    let temp_dir = TempDir::new().expect("temp dir should be created");
    let _guard = CurrentDirGuard::change_to(temp_dir.path());

    generate_models(parsed);

    let user_file =
        fs::read_to_string(temp_dir.path().join("dinoco/models/user.rs")).expect("generated user model should exist");
    let enums_file =
        fs::read_to_string(temp_dir.path().join("dinoco/models/enums.rs")).expect("generated enums file should exist");

    assert!(user_file.contains("Deserialize, Rowable, Default)]"));
    assert!(!user_file.contains("impl Default for User {"));
    assert!(user_file.contains("#[derive(Default)]\npub struct UserInclude {}"));
    assert!(user_file.contains("#[derive(Default)]\npub struct UserRelations {}"));
    assert!(!user_file.contains("impl Default for UserInclude {"));
    assert!(!user_file.contains("impl Default for UserRelations {"));

    assert!(enums_file.contains("PartialEq, Eq, Default, dinoco::serde::Serialize"));
    assert!(enums_file.contains("#[default]\n    #[serde(rename = \"USER\")]"));
    assert!(!enums_file.contains("impl Default for Role {"));
}

#[test]
fn generate_models_preserves_manual_default_when_schema_requires_custom_defaults() {
    let _lock = lock_current_dir();
    let raw = r#"
config {
    database = "sqlite"
    database_url = env("DATABASE_URL")
}

enum Role {
    USER
    ADMIN
}

model User {
    id Integer @id @default(autoincrement())
    name String @default("Dinoco")
    createdAt DateTime @default(now())
    role Role @default(ADMIN)
}
"#;
    let (_, parsed) = compile(raw).expect("schema should compile");
    let temp_dir = TempDir::new().expect("temp dir should be created");
    let _guard = CurrentDirGuard::change_to(temp_dir.path());

    generate_models(parsed);

    let user_file =
        fs::read_to_string(temp_dir.path().join("dinoco/models/user.rs")).expect("generated user model should exist");

    assert!(user_file.contains("Deserialize, Rowable)]"));
    assert!(!user_file.contains("Deserialize, Rowable, Default)]"));
    assert!(user_file.contains("impl Default for User {"));
    assert!(user_file.contains("name: \"Dinoco\".to_string()"));
    assert!(user_file.contains("createdAt: dinoco::Utc::now()"));
    assert!(user_file.contains("role: super::enums::Role::ADMIN"));
}

#[test]
fn generate_models_uses_partitioned_loader_for_many_to_many_include_limits() {
    let _lock = lock_current_dir();
    let raw = r#"
config {
    database = "sqlite"
    database_url = env("DATABASE_URL")
}

model User {
    id String @id @default(uuid())
    name String
    posts Post[] @relation(name: "UserPosts")
}

model Post {
    id String @id @default(uuid())
    title String
    users User[] @relation(name: "UserPosts")
}
"#;
    let (_, parsed) = compile(raw).expect("schema should compile");
    let temp_dir = TempDir::new().expect("temp dir should be created");
    let _guard = CurrentDirGuard::change_to(temp_dir.path());

    generate_models(parsed);

    let model_file =
        fs::read_to_string(temp_dir.path().join("dinoco/models/user.rs")).expect("generated user model should exist");

    assert!(model_file.contains("DinocoPartitionedChildRow"));
    assert!(model_file.contains("build_partitioned_select"));
    assert!(model_file.contains("INNER JOIN"));
    assert!(model_file.contains("format!(\"{}.{}\", \"_UserPosts\""));
}

#[test]
fn generate_models_resolves_complex_named_relations() {
    let _lock = lock_current_dir();
    let raw = r#"
config {
    database = "sqlite"
    database_url = env("DATABASE_URL")
}

enum Role {
    ADMIN
    USER
}

model User {
    id Integer @id @default(autoincrement())
    username String @unique
    role Role @default(USER)

    profile Profile? @relation(name: "UserProfile")

    followers User[] @relation(name: "UserFollows")
    following User[] @relation(name: "UserFollows")

    posts Post[] @relation(name: "PostAuthor")
    comments Comment[] @relation(name: "CommentAuthor")

    likedPosts Post[] @relation(name: "PostLikers")
    likedComments Comment[] @relation(name: "CommentLikers")
}

model Profile {
    id Integer @id @default(autoincrement())
    bio String?
    userId Integer @unique
    user User @relation(name: "UserProfile", fields: [userId], references: [id])
}

model Post {
    id Integer @id @default(autoincrement())
    title String
    content String

    authorId Integer
    author User @relation(name: "PostAuthor", fields: [authorId], references: [id])

    likers User[] @relation(name: "PostLikers")

    comments Comment[]
    tags Tag[]
}

model Comment {
    id Integer @id @default(autoincrement())
    text String

    parentId Integer?
    parent Comment? @relation(name: "CommentReplies", fields: [parentId], references: [id])
    replies Comment[] @relation(name: "CommentReplies")

    postId Integer
    post Post @relation(fields: [postId], references: [id])

    authorId Integer
    author User @relation(name: "CommentAuthor", fields: [authorId], references: [id])

    likers User[] @relation(name: "CommentLikers")
}

model Tag {
    id Integer @id @default(autoincrement())
    name String @unique

    posts Post[]
}
"#;
    let (_, parsed) = compile(raw).expect("schema should compile");
    let temp_dir = TempDir::new().expect("temp dir should be created");
    let _guard = CurrentDirGuard::change_to(temp_dir.path());

    generate_models(parsed);

    let user_file =
        fs::read_to_string(temp_dir.path().join("dinoco/models/user.rs")).expect("generated user model should exist");
    let comment_file = fs::read_to_string(temp_dir.path().join("dinoco/models/comment.rs"))
        .expect("generated comment model should exist");
    let post_file =
        fs::read_to_string(temp_dir.path().join("dinoco/models/post.rs")).expect("generated post model should exist");

    assert!(user_file.contains("pub fn __dinoco_load_profile"));
    assert!(user_file.contains("qualify_select_statement"));
    assert!(user_file.contains("LEFT JOIN"));
    assert!(user_file.contains("client.read_adapter(matches!(read_mode, dinoco::ReadMode::Primary))"));
    assert!(user_file.contains("Expression::Column(format!(\"{}.{}\", \"User\", \"id\"))"));
    assert!(user_file.contains("row.get_optional::<i64>(relation_offset)?"));

    assert!(user_file.contains("pub fn __dinoco_load_followers"));
    assert!(user_file.contains(".condition(dinoco::Expression::Column(\"user_A_id\".to_string())"));
    assert!(user_file.contains("grouped.entry(join_row.user_A_id.clone()).or_default().push(child.clone());"));

    assert!(user_file.contains("pub fn __dinoco_load_following"));
    assert!(user_file.contains(".condition(dinoco::Expression::Column(\"user_B_id\".to_string())"));
    assert!(user_file.contains("grouped.entry(join_row.user_B_id.clone()).or_default().push(child.clone());"));

    assert!(post_file.contains("pub fn __dinoco_load_likers"));
    assert!(post_file.contains(".from(\"_PostLikers\")"));
    assert!(post_file.contains(".condition(dinoco::Expression::Column(\"post_id\".to_string())"));
    assert!(post_file.contains("pub fn __dinoco_load_tags"));
    assert!(post_file.contains(".from(\"_PostTags\")"));
    assert!(post_file.contains(".condition(dinoco::Expression::Column(\"post_id\".to_string())"));
    assert!(post_file.contains("pub enum PostConnection"));
    assert!(post_file.contains("Tag(i64)"));
    assert!(post_file.contains("impl dinoco::InsertConnectionPayload<Post> for PostConnection"));
    assert!(post_file.contains("Self::Tag(value) => vec![dinoco::RelationLinkPlan"));
    assert!(user_file.contains("pub fn __dinoco_count_posts"));
    assert!(user_file.contains("C::load_counts(&mut children, &include.counts, client, read_mode).await?;"));
    assert!(post_file.contains("pub fn __dinoco_count_comments"));
    assert!(post_file.contains("pub fn __dinoco_count_tags"));

    assert!(comment_file.contains("pub fn __dinoco_load_replies"));
    assert!(comment_file.contains("Expression::Column(\"parentId\".to_string())"));
    assert!(comment_file.contains("pub fn __dinoco_load_parent"));
    assert!(comment_file.contains("item_keys: Vec<Option<i64>>"));
    assert!(comment_file.contains("relation_key: i64"));
    assert!(comment_file.contains("Expression::Column(\"id\".to_string()).in_values"));
    assert!(user_file.contains("return Ok(Box::new(move |items: &mut [P]| {"));

    let join_file = fs::read_to_string(temp_dir.path().join("dinoco/models/post_tags.rs"))
        .expect("generated join model should exist");

    assert!(join_file.contains("pub struct PostTags"));
    assert!(join_file.contains("fn table_name() -> &'static str"));
    assert!(join_file.contains("\"_PostTags\""));
}

#[test]
fn generate_models_preserves_enum_variants_with_underscores() {
    let _lock = lock_current_dir();
    let raw = r#"
config {
    database = "sqlite"
    database_url = env("DATABASE_URL")
}

enum Status {
    IN_PROGRESS
    DONE
}

model Task {
    id String @id @default(uuid())
    status Status @default(IN_PROGRESS)
}
"#;
    let (_, parsed) = compile(raw).expect("schema should compile");
    let temp_dir = TempDir::new().expect("temp dir should be created");
    let _guard = CurrentDirGuard::change_to(temp_dir.path());

    generate_models(parsed);

    let enum_file =
        fs::read_to_string(temp_dir.path().join("dinoco/models/enums.rs")).expect("generated enum file should exist");
    let model_file =
        fs::read_to_string(temp_dir.path().join("dinoco/models/task.rs")).expect("generated task model should exist");

    assert!(enum_file.contains("pub enum Status"));
    assert!(
        enum_file
            .contains("#[derive(Debug, Clone, PartialEq, Eq, dinoco::serde::Serialize, dinoco::serde::Deserialize)]")
    );
    assert!(enum_file.contains("#[serde(crate = \"dinoco::serde\")]"));
    assert!(enum_file.contains("    #[serde(rename = \"IN_PROGRESS\")]"));
    assert!(enum_file.contains("    IN_PROGRESS,"));
    assert!(enum_file.contains("        Self::IN_PROGRESS"));
    assert!(enum_file.contains("\"IN_PROGRESS\" => Ok(Self::IN_PROGRESS)"));
    assert!(model_file.contains("status: super::enums::Status::IN_PROGRESS"));
}

#[test]
fn generate_models_deduplicates_relation_imports_and_skips_self_imports() {
    let _lock = lock_current_dir();
    let raw = r#"
config {
    database = "sqlite"
    database_url = env("DATABASE_URL")
}

model User {
    id Integer @id @default(autoincrement())
    managerId Integer?
    manager User? @relation(name: "UserManager", fields: [managerId], references: [id])
    reports User[] @relation(name: "UserManager")
    posts Post[] @relation(name: "PostAuthor")
    editedPosts Post[] @relation(name: "PostEditor")
    comments Comment[] @relation(name: "CommentAuthor")
    reviewedComments Comment[] @relation(name: "CommentReviewer")
}

model Post {
    id Integer @id @default(autoincrement())
    authorId Integer
    editorId Integer
    author User @relation(name: "PostAuthor", fields: [authorId], references: [id])
    editor User @relation(name: "PostEditor", fields: [editorId], references: [id])
}

model Comment {
    id Integer @id @default(autoincrement())
    authorId Integer
    reviewerId Integer
    author User @relation(name: "CommentAuthor", fields: [authorId], references: [id])
    reviewer User @relation(name: "CommentReviewer", fields: [reviewerId], references: [id])
}
"#;
    let (_, parsed) = compile(raw).expect("schema should compile");
    let temp_dir = TempDir::new().expect("temp dir should be created");
    let _guard = CurrentDirGuard::change_to(temp_dir.path());

    generate_models(parsed);

    let user_file =
        fs::read_to_string(temp_dir.path().join("dinoco/models/user.rs")).expect("generated user model should exist");
    let post_file =
        fs::read_to_string(temp_dir.path().join("dinoco/models/post.rs")).expect("generated post model should exist");

    assert!(!user_file.contains("use super::{comment::Comment, post::Post, user::User};"));
    assert!(!user_file.contains("use super::{comment::Comment, post::Post, post::Post};"));
    assert!(user_file.contains("use super::{comment::Comment, post::Post};"));
    assert!(post_file.contains("use super::{user::User};"));
    assert!(!post_file.contains("use super::{user::User, user::User};"));
}
