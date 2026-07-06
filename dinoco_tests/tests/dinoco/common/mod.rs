use std::env;
use std::sync::OnceLock;

use dinoco_engine::DinocoError;
use tokio::sync::{Mutex, MutexGuard};
use uuid::Uuid;

static MYSQL_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
static POSTGRES_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

pub fn postgres_url() -> String {
    database_url(
        &["DINOCO_POSTGRES_DATABASE_URL", "POSTGRES_DATABASE_URL"],
        "postgres://postgres:root@localhost:5432/dinoco",
    )
}

pub fn mysql_url() -> String {
    database_url(&["DINOCO_MYSQL_DATABASE_URL", "MYSQL_DATABASE_URL"], "mysql://root:root@localhost:3306/dinoco")
}

pub fn sqlite_url(name: &str) -> String {
    let mut path = env::temp_dir();

    path.push(format!("dinoco-core-{name}-{}.sqlite", Uuid::now_v7()));

    format!("file:{}", path.display())
}

pub async fn lock_postgres() -> MutexGuard<'static, ()> {
    POSTGRES_LOCK.get_or_init(|| Mutex::new(())).lock().await
}

pub async fn lock_mysql() -> MutexGuard<'static, ()> {
    MYSQL_LOCK.get_or_init(|| Mutex::new(())).lock().await
}

fn database_url(keys: &[&str], default: &str) -> String {
    keys.iter()
        .find_map(|key| env::var(key).ok())
        .or_else(|| env::var("DATABASE_URL").ok())
        .unwrap_or_else(|| default.to_string())
}

pub fn should_skip_external_adapter_test(error: &DinocoError) -> bool {
    match error {
        DinocoError::ConnectionError(_) => true,
        DinocoError::MySql(mysql_error) => mysql_error.to_string().contains("Operation not permitted"),
        DinocoError::Postgres(postgres_error) => postgres_error.to_string().contains("error connecting to server"),
        _ => false,
    }
}
