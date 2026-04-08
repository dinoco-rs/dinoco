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

fn expect_compile_error(raw: &str, expected: &str) {
    let errors = compile(raw).expect_err("schema should fail");

    assert!(
        errors.iter().any(|error| error.message.contains(expected)),
        "expected one error containing '{expected}', got: {errors:#?}"
    );
}

fn normalize_relation_name(name: &str) -> &str {
    name.trim_matches('"')
}

#[test]
fn compile_parses_table_name_composite_ids_and_read_replicas() {
    let raw = r#"
config {
    database = "postgresql"
    database_url = "postgresql://primary"
    read_replicas = ["postgresql://replica-a", env("REPLICA_B")]
}

model Membership {
    userId Integer
    teamId Integer

    @@ids([userId, teamId])
    @@table_name("memberships")
}
"#;

    let (_, parsed) = compile(raw).expect("schema should compile");
    let table = &parsed.tables[0];

    assert_eq!(parsed.config.read_replicas.len(), 2);
    assert_eq!(table.database_name, "memberships");
    assert_eq!(table.primary_key_fields, vec!["userId".to_string(), "teamId".to_string()]);
}

#[test]
fn compile_rejects_duplicate_database_key() {
    let raw = r#"
config {
    database = "sqlite"
    database = "postgresql"
    database_url = "file:dev.db"
}
"#;

    expect_compile_error(raw, "Duplicate 'database'.");
}

#[test]
fn compile_rejects_invalid_read_replica_url() {
    let raw = r#"
config {
    database = "sqlite"
    database_url = "file:dev.db"
    read_replicas = ["replica.db"]
}
"#;

    expect_compile_error(raw, "Replica connection URLS must start with a valid protocol.");
}

#[test]
fn compile_rejects_duplicate_model_table_name_decorator() {
    let raw = r#"
config {
    database = "sqlite"
    database_url = "file:dev.db"
}

model User {
    id Integer @id

    @@table_name("users")
    @@table_name("people")
}
"#;

    expect_compile_error(raw, "Duplicate '@@table_name'.");
}

#[test]
fn compile_rejects_field_id_together_with_composite_ids() {
    let raw = r#"
config {
    database = "sqlite"
    database_url = "file:dev.db"
}

model Membership {
    userId Integer @id
    teamId Integer

    @@ids([userId, teamId])
}
"#;

    expect_compile_error(raw, "Field-level @id cannot be used together with @@ids([...]).");
}

#[test]
fn compile_rejects_autoincrement_on_non_primary_key() {
    let raw = r#"
config {
    database = "sqlite"
    database_url = "file:dev.db"
}

model User {
    id Integer @id
    seq Integer @default(autoincrement())
}
"#;

    expect_compile_error(raw, "Autoincrement is only supported on primary key fields");
}

#[test]
fn compile_rejects_now_default_on_non_datetime_field() {
    let raw = r#"
config {
    database = "sqlite"
    database_url = "file:dev.db"
}

model Post {
    id Integer @id
    publishedAt String @default(now())
}
"#;

    expect_compile_error(raw, "now() is only supported for DateTime fields.");
}

#[test]
fn compile_rejects_relation_reference_outside_target_primary_key() {
    let raw = r#"
config {
    database = "sqlite"
    database_url = "file:dev.db"
}

model User {
    id Integer @id
    email String @unique
    posts Post[]
}

model Post {
    id Integer @id
    authorEmail String
    author User @relation(fields: [authorEmail], references: [email])
}
"#;

    expect_compile_error(raw, "must belong to the model primary key");
}

#[test]
fn compile_rejects_many_to_many_with_explicit_fields() {
    let raw = r#"
config {
    database = "sqlite"
    database_url = "file:dev.db"
}

model User {
    id Integer @id
    roleId Integer
    roles Role[] @relation(fields: [roleId], references: [id])
}

model Role {
    id Integer @id
    users User[]
}
"#;

    expect_compile_error(raw, "Many-to-Many relations cannot define 'fields' or 'references'.");
}

#[test]
fn compile_rejects_invalid_referential_action() {
    let raw = r#"
config {
    database = "sqlite"
    database_url = "file:dev.db"
}

model User {
    id Integer @id
    posts Post[]
}

model Post {
    id Integer @id
    authorId Integer
    author User @relation(fields: [authorId], references: [id], onDelete: Restrict)
}
"#;

    expect_compile_error(raw, "Valor inválido para onDelete: 'Restrict'");
}

#[test]
fn compile_parses_named_one_to_one_relation() {
    let raw = r#"
config {
    database = "sqlite"
    database_url = "file:dev.db"
}

model User {
    id Integer @id
    profile Profile? @relation(name: "UserProfile")
}

model Profile {
    id Integer @id
    userId Integer @unique
    user User @relation(name: "UserProfile", fields: [userId], references: [id], onDelete: Cascade, onUpdate: SetNull)
}
"#;

    let (_, parsed) = compile(raw).expect("schema should compile");
    let user_table = parsed.tables.iter().find(|table| table.name == "User").unwrap();
    let profile_table = parsed.tables.iter().find(|table| table.name == "Profile").unwrap();
    let user_profile = user_table.fields.iter().find(|field| field.name == "profile").unwrap();
    let profile_user = profile_table.fields.iter().find(|field| field.name == "user").unwrap();

    match &user_profile.relation {
        dinoco_compiler::ParsedRelation::OneToOneInverse(Some(name)) => {
            assert_eq!(normalize_relation_name(name), "UserProfile");
        }
        relation => panic!("unexpected inverse relation: {relation:?}"),
    }

    match &profile_user.relation {
        dinoco_compiler::ParsedRelation::OneToOneOwner(name, fields, references, on_delete, on_update) => {
            assert_eq!(normalize_relation_name(name.as_deref().unwrap()), "UserProfile");
            assert_eq!(fields, &vec!["userId".to_string()]);
            assert_eq!(references, &vec!["id".to_string()]);
            assert_eq!(*on_delete, Some(dinoco_compiler::ReferentialAction::Cascade));
            assert_eq!(*on_update, Some(dinoco_compiler::ReferentialAction::SetNull));
        }
        relation => panic!("unexpected owner relation: {relation:?}"),
    }
}

#[test]
fn compile_parses_named_self_one_to_many_relation() {
    let raw = r#"
config {
    database = "sqlite"
    database_url = "file:dev.db"
}

model Comment {
    id Integer @id
    parentId Integer?
    parent Comment? @relation(name: "CommentReplies", fields: [parentId], references: [id])
    replies Comment[] @relation(name: "CommentReplies")
}
"#;

    let (_, parsed) = compile(raw).expect("self relation schema should compile");
    let comment_table = parsed.tables.iter().find(|table| table.name == "Comment").unwrap();
    let parent = comment_table.fields.iter().find(|field| field.name == "parent").unwrap();
    let replies = comment_table.fields.iter().find(|field| field.name == "replies").unwrap();

    match &parent.relation {
        dinoco_compiler::ParsedRelation::ManyToOne(Some(name), fields, references, _, _) => {
            assert_eq!(normalize_relation_name(name), "CommentReplies");
            assert_eq!(fields, &vec!["parentId".to_string()]);
            assert_eq!(references, &vec!["id".to_string()]);
        }
        relation => panic!("unexpected parent relation: {relation:?}"),
    }

    match &replies.relation {
        dinoco_compiler::ParsedRelation::OneToMany(Some(name)) => {
            assert_eq!(normalize_relation_name(name), "CommentReplies");
        }
        relation => panic!("unexpected replies relation: {relation:?}"),
    }
}
