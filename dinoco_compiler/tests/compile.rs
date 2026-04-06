use dinoco_compiler::{compile, render_error};

const VALID_SCHEMA: &str = r#"
config {
    database = "sqlite"
    database_url = env("DATABASE_URL")
}

enum UserRole {
    ADMIN
    MEMBER
}

model User {
    id Integer @id @default(autoincrement())
    email String @unique
    role UserRole @default(MEMBER)
}
"#;

#[test]
fn compile_parses_valid_schema() {
    let (_, parsed) = compile(VALID_SCHEMA).expect("valid schema should compile");

    assert_eq!(parsed.tables.len(), 1);
    assert_eq!(parsed.enums.len(), 1);
    assert_eq!(parsed.tables[0].name, "User");
    assert_eq!(parsed.tables[0].fields[1].name, "email");
}

#[test]
fn compile_reports_missing_database_url() {
    let raw = r#"
config {
    database = "sqlite"
}
"#;

    let errors = compile(raw).expect_err("schema should fail without database_url");

    assert!(!errors.is_empty());
    assert!(errors[0].message.contains("database_url"));
}

#[test]
fn render_error_formats_source_context() {
    let raw = r#"
config {
    database = "sqlite"
}
"#;
    let errors = compile(raw).expect_err("schema should fail");
    let rendered = render_error(&errors[0], raw);

    assert!(rendered.contains("database_url"));
    assert!(rendered.contains("schema.dinoco"));
    assert!(rendered.contains("happened here"));
}
