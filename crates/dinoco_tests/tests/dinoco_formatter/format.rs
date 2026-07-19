#[test]
fn formatter_returns_stable_canonical_schema() {
    let raw = r#"
model User{
id String @id @default(uuid())
email String
}

enum OfficeType{admin member}
"#;

    let once = dinoco_formatter::format_from_raw(raw).expect("format once");
    let twice = dinoco_formatter::format_from_raw(&once).expect("format twice");

    assert_eq!(once, twice);
    assert!(once.contains("model User {"));
    assert!(once.contains("enum OfficeType {"));
    assert!(once.ends_with('\n'));
}

#[test]
fn formatter_keeps_env_urls_readable() {
    let raw = r#"
config {
database = "postgresql"
database_url = env("DATABASE_URL")
read_replicas = [env("DATABASE_REPLICA_URL")]
}
"#;

    let formatted = dinoco_formatter::format_from_raw(raw).expect("format");

    assert!(formatted.contains("database_url"));
    assert!(formatted.contains("env(\"DATABASE_URL\")"));
    assert!(formatted.contains("read_replicas = ["));
    assert!(formatted.contains("env(\"DATABASE_REPLICA_URL\")"));
}
