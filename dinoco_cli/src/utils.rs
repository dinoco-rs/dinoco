use std::env;
use std::path::Path;

#[macro_export]
macro_rules! ternary {
    ($cond:expr, $a:expr, $b:expr) => {
        if $cond { $a } else { $b }
    };
}

pub fn env_prompt_bool(key: &str) -> Option<bool> {
    let value = env::var(key).ok()?;

    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "y" => Some(true),
        "0" | "false" | "no" | "n" => Some(false),
        _ => None,
    }
}

pub fn env_prompt_string(key: &str) -> Option<String> {
    env::var(key).ok().map(|value| value.trim().to_string()).filter(|value| !value.is_empty())
}

pub fn normalize_sqlite_database_url(url: &str, base_dir: &str) -> String {
    if !url.starts_with("file:") {
        return url.to_string();
    }

    let raw = &url["file:".len()..];

    if raw.is_empty() || raw == ":memory:" || raw.starts_with('/') || raw.starts_with("//") || is_windows_absolute(raw)
    {
        return url.to_string();
    }

    let (path_part, suffix) = match raw.split_once('?') {
        Some((path, query)) => (path, format!("?{query}")),
        None => (raw, String::new()),
    };

    if path_part == ":memory:" || path_part.starts_with(&format!("{base_dir}/")) {
        return url.to_string();
    }

    let dot_prefixed_base = format!("./{base_dir}/");
    if path_part.starts_with(&dot_prefixed_base) {
        return url.to_string();
    }

    let normalized_relative = path_part.strip_prefix("./").unwrap_or(path_part);
    let normalized_path = Path::new(base_dir).join(normalized_relative);
    let normalized_path = normalized_path.to_string_lossy().replace('\\', "/");

    format!("file:{normalized_path}{suffix}")
}

fn is_windows_absolute(path: &str) -> bool {
    let bytes = path.as_bytes();

    bytes.len() > 2 && bytes[1] == b':' && (bytes[2] == b'/' || bytes[2] == b'\\') && bytes[0].is_ascii_alphabetic()
}

#[cfg(test)]
mod tests {
    use super::normalize_sqlite_database_url;

    #[test]
    fn keeps_non_sqlite_urls_untouched() {
        assert_eq!(
            normalize_sqlite_database_url("postgresql://localhost:5432/db", "dinoco"),
            "postgresql://localhost:5432/db"
        );
    }

    #[test]
    fn keeps_absolute_sqlite_paths_untouched() {
        assert_eq!(normalize_sqlite_database_url("file:/tmp/dinoco.sqlite", "dinoco"), "file:/tmp/dinoco.sqlite");
    }

    #[test]
    fn rebases_relative_sqlite_paths_to_base_dir() {
        assert_eq!(normalize_sqlite_database_url("file:dev.sqlite", "dinoco"), "file:dinoco/dev.sqlite");
        assert_eq!(normalize_sqlite_database_url("file:./db/dev.sqlite", "dinoco"), "file:dinoco/db/dev.sqlite");
        assert_eq!(
            normalize_sqlite_database_url("file:db/dev.sqlite?mode=ro", "dinoco"),
            "file:dinoco/db/dev.sqlite?mode=ro"
        );
    }
}
