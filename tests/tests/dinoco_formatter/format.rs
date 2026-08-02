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
fn formatter_formats_logger_and_pool_sizes() {
    let formatted = dinoco_formatter::format_from_raw(
        r#"config { database="postgresql" database_url=env("DATABASE_URL") with_logger=true min_connection=2 max_connection=10 }"#,
    )
    .expect("format config values");

    assert!(formatted.contains("with_logger    = true"));
    assert!(formatted.contains("min_connection = 2"));
    assert!(formatted.contains("max_connection = 10"));
    assert_eq!(formatted, dinoco_formatter::format_from_raw(&formatted).expect("format must be stable"));
}

#[test]
fn formatter_formats_workspace_configs() {
    let raw = r#"
config { workspace { dev { database="sqlite" database_url=env("DEV_DATABASE_URL") } prod { database="postgresql" database_url=env("PROD_DATABASE_URL") } } }
"#;

    let formatted = dinoco_formatter::format_from_raw(raw).expect("format workspaces");

    assert_eq!(
        formatted,
        r#"config {
    workspace {
        dev {
            database     = "sqlite"
            database_url = env("DEV_DATABASE_URL")
        }

        prod {
            database     = "postgresql"
            database_url = env("PROD_DATABASE_URL")
        }
    }
}
"#
    );
    assert_eq!(formatted, dinoco_formatter::format_from_raw(&formatted).expect("workspace format is stable"));
}

#[test]
fn formatter_aligns_types_inside_each_contiguous_field_group() {
    let raw = r#"
model Teste {
id String @id

document String
email String
}
"#;

    let formatted = dinoco_formatter::format_from_raw(raw).expect("format");

    assert_eq!(
        formatted,
        r#"model Teste {
    id  String  @id

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
id String @id


document String
email String
}
"#;

    let once = dinoco_formatter::format_from_raw(raw).expect("format once");
    let twice = dinoco_formatter::format_from_raw(&once).expect("format twice");

    assert_eq!(once, twice);
    assert_eq!(once.matches("\n\n").count(), 1);
}

#[test]
fn formatter_moves_model_attributes_after_all_fields() {
    let raw = r#"
model Article {
@@indexes([tenantId,id])
tenantId String
@@fulltexts([title,body])
id String
title String
@@uniques([tenantId,id])
body String?
@@ids([tenantId,id])
}
"#;

    let formatted = dinoco_formatter::format_from_raw(raw).expect("format model attributes");
    let body_position = formatted.find("body").expect("body");
    let ids_position = formatted.find("@@ids").expect("@@ids");
    let uniques_position = formatted.find("@@uniques").expect("@@uniques");
    let indexes_position = formatted.find("@@indexes").expect("@@indexes");
    let fulltexts_position = formatted.find("@@fulltexts").expect("@@fulltexts");

    assert!(body_position < indexes_position);
    assert!(body_position < fulltexts_position);
    assert!(body_position < uniques_position);
    assert!(body_position < ids_position);
    assert!(formatted.contains("\n\n    @@indexes([tenantId, id])"));
    assert_eq!(formatted, dinoco_formatter::format_from_raw(&formatted).expect("idempotent format"));
}
