use std::env;

use uuid::Uuid;

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

fn database_url(keys: &[&str], default: &str) -> String {
    keys.iter()
        .find_map(|key| env::var(key).ok())
        .or_else(|| env::var("DATABASE_URL").ok())
        .unwrap_or_else(|| default.to_string())
}
