use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

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
    let _lock = current_dir_lock().lock().expect("current dir lock should be acquired");
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

    assert!(model_file.contains("pub struct Post"));
    assert!(model_file.contains("fn validate_insert(&self) -> dinoco::DinocoResult<()>"));
    assert!(model_file.contains("Field 'Post.name' is required for insert and cannot be empty"));
    assert!(model_file.contains("impl Model for Post"));
}

#[test]
fn generate_models_uses_partitioned_loader_for_many_to_many_include_limits() {
    let _lock = current_dir_lock().lock().expect("current dir lock should be acquired");
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
