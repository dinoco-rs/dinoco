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
