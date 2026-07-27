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

#[test]
fn formatter_aligns_types_inside_each_contiguous_field_group() {
    let raw = r#"
model Teste {
id String

document String
email String
}
"#;

    let formatted = dinoco_formatter::format_from_raw(raw).expect("format");

    assert_eq!(
        formatted,
        r#"model Teste {
    id  String

    document  String
    email     String
}
"#
    );
}

#[test]
fn formatter_does_not_use_a_later_group_to_pad_an_earlier_group() {
    let raw = r#"
model Teste {
id String @id
name String

very_long_document String @unique
email String @default("test@example.com")
}
"#;

    let formatted = dinoco_formatter::format_from_raw(raw).expect("format");

    assert_eq!(
        formatted,
        r#"model Teste {
    id    String  @id
    name  String

    very_long_document  String  @unique
    email               String  @default("test@example.com")
}
"#
    );
}

#[test]
fn formatter_is_idempotent_with_multiple_field_groups() {
    let raw = r#"
model Teste {
id String


document String
email String
}
"#;

    let once = dinoco_formatter::format_from_raw(raw).expect("format once");
    let twice = dinoco_formatter::format_from_raw(&once).expect("format twice");

    assert_eq!(once, twice);
    assert_eq!(once.matches("\n\n").count(), 1);
}
