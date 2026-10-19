use dinoco_vscode::document::{DocumentIndex, ResolvedSymbol};

const SCHEMA: &str = r#"config {
    database = "postgresql"
    database_url = env("DATABASE_URL")
}

enum Role { USER ADMIN }

model User {
    id String @id @default(uuid())
    manager User? @relation(name: "management", fields: [manager_id], references: [id])
    manager_id String?
    role Role @default(USER)
}
"#;

#[test]
fn indexes_blocks_fields_and_attributes() {
    let index = DocumentIndex::new(SCHEMA);
    assert_eq!(index.blocks.len(), 3);
    let user = index.model("User").expect("user model");
    assert_eq!(user.fields.len(), 4);
    let manager = user.field("manager").expect("manager");
    assert!(manager.optional);
    let relation = manager.attribute("relation").expect("relation");
    assert_eq!(relation.argument("fields").expect("fields").values[0].name, "manager_id");
}

#[test]
fn resolves_relation_and_enum_references() {
    let index = DocumentIndex::new(SCHEMA);
    let user = index.model("User").expect("user");
    let manager = user.field("manager").expect("manager");
    let reference =
        manager.attribute("relation").expect("relation").argument("references").expect("references").values[0].range;
    assert_eq!(
        index.resolve_symbol(reference.start),
        Some(ResolvedSymbol::Field { model: "User".into(), field: "id".into() })
    );

    let role = user.field("role").expect("role");
    let value = role.attribute("default").expect("default").arguments[0].values[0].range;
    assert_eq!(
        index.resolve_symbol(value.start),
        Some(ResolvedSymbol::EnumValue { enum_name: "Role".into(), value: "USER".into() })
    );
}

#[test]
fn type_occurrences_include_named_imports_without_mixing_external_ranges() {
    let source = "import { Account } from \"account.dinoco\"\nmodel Session { id String @id account Account }";
    let local = DocumentIndex::new(source);
    let imported = DocumentIndex::new("model Account { id String @id }");
    let index = local.with_external_declarations(imported.blocks);
    let account_type = index.model("Session").expect("session").field("account").expect("account").ty.range;

    assert_eq!(index.resolve_symbol(account_type.start), Some(ResolvedSymbol::Type("Account".to_string())));
    assert_eq!(index.occurrences(&ResolvedSymbol::Type("Account".to_string())).len(), 2);
}

const CONDITIONS: &str = r#"config {
    database = "postgresql"
    database_url = env("DATABASE_URL")
}

enum Plan { FREE PRO }

model Account {
    id String @id
    email String? @unique<email != null> @index
    score Integer @index<score > 0 && score >= 1>(map: "account_score_key")
    plan Plan
    deleted_at DateTime?

    @@uniques<deleted_at == null && plan == PRO>([email, plan])
}
"#;

#[test]
fn conditions_are_parsed_without_swallowing_the_next_fields() {
    let index = DocumentIndex::new(CONDITIONS);
    let account = index.model("Account").expect("account");
    assert_eq!(
        account.fields.iter().map(|field| field.name.name.as_str()).collect::<Vec<_>>(),
        ["id", "email", "score", "plan", "deleted_at"]
    );

    let email = account.field("email").expect("email");
    let unique = email.attribute("unique").expect("unique");
    let names = |symbols: &[dinoco_vscode::document::Symbol]| {
        symbols.iter().map(|symbol| symbol.name.clone()).collect::<Vec<_>>()
    };
    assert_eq!(names(unique.condition.as_deref().expect("condition")), ["email"]);
    assert!(email.attribute("index").expect("index").condition.is_none(), "attributes after a condition parse");

    // `>` and `>=` inside the condition don't close it; arguments follow it.
    let score = account.field("score").expect("score").attribute("index").expect("index");
    assert_eq!(names(score.condition.as_deref().expect("condition")), ["score", "score"]);
    assert_eq!(score.argument("map").expect("map").values[0].name, "account_score_key");

    let uniques = account.attribute("uniques").expect("uniques");
    assert_eq!(names(uniques.condition.as_deref().expect("condition")), ["deleted_at", "plan", "PRO"]);
}

#[test]
fn condition_fields_resolve_and_rename_with_their_field() {
    let index = DocumentIndex::new(CONDITIONS);
    let account = index.model("Account").expect("account");
    let condition = account.attribute("uniques").expect("uniques").condition.clone().expect("condition");
    let deleted_at = ResolvedSymbol::Field { model: "Account".into(), field: "deleted_at".into() };

    assert_eq!(index.resolve_symbol(condition[0].range.start), Some(deleted_at.clone()));
    // An enum value is not a field.
    assert_eq!(index.resolve_symbol(condition[2].range.start), None);

    let occurrences = index.occurrences(&deleted_at);
    assert!(occurrences.contains(&condition[0].range), "{occurrences:?}");
    assert!(occurrences.contains(&account.field("deleted_at").expect("field").name.range));

    let score = ResolvedSymbol::Field { model: "Account".into(), field: "score".into() };
    assert_eq!(index.occurrences(&score).len(), 3);
}
