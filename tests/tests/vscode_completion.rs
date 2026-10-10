use dinoco_vscode::completion::{ImportCompletionContext, complete, import_completion_context};
use dinoco_vscode::diagnostics::analyze;
use dinoco_vscode::document::DocumentIndex;
use dinoco_vscode::tower_lsp::lsp_types::{CompletionResponse, NumberOrString, Position, Range};

#[test]
fn schema_completion_exposes_standard_and_fulltext_indexes() {
    let source = "model Account {\n    biography String @\n}";
    let index = DocumentIndex::new(source);
    let CompletionResponse::Array(items) = complete(source, &index, Position::new(1, 22)) else {
        panic!("completion should return an item array");
    };

    assert!(items.iter().any(|item| item.label == "@index"));
    assert!(items.iter().any(|item| item.label == "@fulltext"));
    assert!(items.iter().any(|item| item.label == "@updated_at"));
}

#[test]
fn schema_completion_indexes_and_resolves_model_attributes() {
    let source = "model Account {\n    id String @id\n    name String\n    document String\n    @@\n}";
    let index = DocumentIndex::new(source);
    let CompletionResponse::Array(items) = complete(source, &index, Position::new(4, 6)) else {
        panic!("completion should return an item array");
    };
    for label in ["@@ids", "@@uniques", "@@indexes", "@@fulltexts", "@@table_name"] {
        assert!(items.iter().any(|item| item.label == label), "missing {label}");
    }

    let source = "model Account {\n    id String @id\n    name String\n    document String\n    @@indexes([\n}";
    let index = DocumentIndex::new(source);
    let CompletionResponse::Array(items) = complete(source, &index, Position::new(4, 15)) else {
        panic!("field completion should return an item array");
    };
    assert!(items.iter().any(|item| item.label == "id"));
    assert!(items.iter().any(|item| item.label == "name"));
    assert!(items.iter().any(|item| item.label == "document"));

    let indexed = DocumentIndex::new(
        "model Account {\n    id String\n    name String\n    @@ids([id, name])\n    @@indexes([name])\n}",
    );
    let account = indexed.model("Account").expect("model");
    assert!(account.attribute("ids").is_some());
    assert!(account.attribute("indexes").is_some());
}

#[test]
fn diagnostics_report_missing_and_multiple_primary_keys() {
    let missing = "model Account {\n    email String\n}";
    let missing_index = DocumentIndex::new(missing);
    let diagnostics = analyze(missing, &missing_index);
    assert!(
        diagnostics
            .iter()
            .any(|item| { item.code == Some(NumberOrString::String("dinoco.missingPrimaryKey".to_string())) })
    );

    let repeated = "model Account {\n    id String @id\n    legacy String @id\n}";
    let repeated_index = DocumentIndex::new(repeated);
    let diagnostics = analyze(repeated, &repeated_index);
    assert!(
        diagnostics
            .iter()
            .any(|item| { item.code == Some(NumberOrString::String("dinoco.multiplePrimaryKeys".to_string())) })
    );
}

#[test]
fn config_completion_exposes_logger_and_postgres_pool_settings() {
    let source = "config {\n    \n}";
    let index = DocumentIndex::new(source);
    let CompletionResponse::Array(items) = complete(source, &index, Position::new(1, 4)) else {
        panic!("config completion should return an item array");
    };

    for label in ["with_logger", "min_connection", "max_connection"] {
        assert!(items.iter().any(|item| item.label == label), "missing {label}");
    }
}

#[test]
fn diagnostics_report_ambiguous_and_invalid_pool_configs() {
    let mixed = r#"
config {
    database = "postgresql"
    database_url = env("DATABASE_URL")
    workspace {
        dev {
            database = "postgresql"
            database_url = env("DEV_DATABASE_URL")
        }
    }
}
"#;
    let diagnostics = analyze(mixed, &DocumentIndex::new(mixed));
    assert!(
        diagnostics
            .iter()
            .filter(|item| item.code == Some(NumberOrString::String("dinoco.ambiguousConfig".to_string())))
            .count()
            >= 2,
        "{diagnostics:#?}"
    );

    let invalid_pool = r#"
config {
    database = "postgresql"
    database_url = env("DATABASE_URL")
    with_logger = true
    min_connection = 20
    max_connection = 10
}
"#;
    let diagnostics = analyze(invalid_pool, &DocumentIndex::new(invalid_pool));
    assert!(
        diagnostics
            .iter()
            .any(|item| { item.code == Some(NumberOrString::String("dinoco.invalidPoolRange".to_string())) })
    );
}

#[test]
fn diagnostics_require_a_now_default_for_updated_at() {
    let schema = |field: &str| {
        format!(
            "config {{\n    database = \"sqlite\"\n    database_url = env(\"DATABASE_URL\")\n}}\n\nmodel Article {{\n    id String @id\n    {field}\n}}\n"
        )
    };

    let valid = schema("updated_at DateTime @updated_at @default(now())");
    let diagnostics = analyze(&valid, &DocumentIndex::new(&valid));
    assert!(diagnostics.is_empty(), "{diagnostics:#?}");

    let missing = schema("updated_at DateTime @updated_at");
    let diagnostics = analyze(&missing, &DocumentIndex::new(&missing));
    assert!(
        diagnostics.iter().any(|item| item.code == Some(NumberOrString::String("dinoco.schema".to_string()))
            && item.message.contains("@updated_at on `Article.updated_at` requires @default(now())")),
        "{diagnostics:#?}"
    );
}

#[test]
fn suggests_target_fields_inside_references() {
    let source = r#"model User { id String @id }
model Token {
    user User? @relation(fields: [user_id], references: [i
    user_id String?
}"#;
    let index = DocumentIndex::new(source);
    let response = complete(source, &index, Position::new(2, 65));
    let CompletionResponse::Array(items) = response else {
        panic!("array response");
    };
    assert!(items.iter().any(|item| item.label == "id"));
}

#[test]
fn suggests_enum_defaults() {
    let source = "enum Role { USER ADMIN }\nmodel User { role Role @default( }";
    let index = DocumentIndex::new(source);
    let cursor = source.lines().nth(1).expect("model line").encode_utf16().count() as u32 - 1;
    let response = complete(source, &index, Position::new(1, cursor));
    let CompletionResponse::Array(items) = response else {
        panic!("array response");
    };
    assert!(items.iter().any(|item| item.label == "USER"));
}

#[test]
fn suggests_main_schema_config_imports() {
    let source = "config {\n    \n}";
    let index = DocumentIndex::new(source);
    let response = complete(source, &index, Position::new(1, 4));
    let CompletionResponse::Array(items) = response else {
        panic!("array response");
    };

    let imports = items.iter().find(|item| item.label == "imports").expect("imports completion");
    assert_eq!(imports.insert_text.as_deref(), Some("imports = [\"${1:models/account.dinoco}\"]"));
}

#[test]
fn imported_declarations_participate_in_model_type_completion() {
    let source = "model Account {\n    session \n}";
    let local = DocumentIndex::new(source);
    let imported = DocumentIndex::new("model Session { id String @id }\nenum SessionState { ACTIVE EXPIRED }");
    let semantic = local.with_external_declarations(imported.blocks);
    let CompletionResponse::Array(items) = complete(source, &semantic, Position::new(1, 12)) else {
        panic!("completion array");
    };

    assert!(items.iter().any(|item| item.label == "Session"));
    assert!(items.iter().any(|item| item.label == "SessionState"));
}

#[test]
fn recognizes_symbol_completion_only_with_a_complete_import_path() {
    let source = "import {  } from \"./entities.dinoco\"";
    assert_eq!(
        import_completion_context(source, Position::new(0, 9)),
        Some(ImportCompletionContext::Symbols { path: Some("./entities.dinoco".to_string()) })
    );

    let incomplete = "import {  } from \"./missing";
    assert_eq!(
        import_completion_context(incomplete, Position::new(0, 9)),
        Some(ImportCompletionContext::Symbols { path: None })
    );
}

#[test]
fn recognizes_import_path_completion_ranges() {
    let source = "import { Account } from \"./ent\"";
    let context = import_completion_context(source, Position::new(0, 30)).expect("import path context");
    assert_eq!(
        context,
        ImportCompletionContext::Path {
            fragment: "./ent".to_string(),
            replace: Range::new(Position::new(0, 25), Position::new(0, 30)),
            quoted: true,
        }
    );
}
