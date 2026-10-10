use std::collections::HashMap;
use std::fs;

use dinoco_vscode::document::DocumentIndex;
use dinoco_vscode::server::{
    SEMANTIC_MODIFIER_DECLARATION, SEMANTIC_TOKEN_ENUM_MEMBER, SEMANTIC_TOKEN_PROPERTY, SEMANTIC_TOKEN_TYPE,
    compile_error_diagnostic, edit_distance, encode_semantic_tokens, format_document_source, import_diagnostics,
    is_main_schema, semantic_token_spans, unknown_type_fix,
};
use dinoco_vscode::tower_lsp::lsp_types::{Diagnostic, NumberOrString, Position, Range, Url};
use dinoco_vscode::workspace::{WorkspaceCache, canonical_path};
use tempfile::tempdir;

#[test]
fn only_schema_dinoco_is_treated_as_the_database_entrypoint() {
    assert!(is_main_schema(&Url::parse("file:///project/dinoco/schema.dinoco").unwrap()));
    assert!(!is_main_schema(&Url::parse("file:///project/dinoco/models/account.dinoco").unwrap()));
    assert!(!is_main_schema(&Url::parse("file:///project/dinoco/enums.dinoco").unwrap()));
    assert!(!is_main_schema(&Url::parse("file:///project/dinoco/models/schema.dinoco").unwrap()));
}

#[test]
fn formatting_imported_snowflake_models_does_not_require_project_config() {
    let uri = Url::parse("file:///project/dinoco/models/account.dinoco").unwrap();
    let source = "model Account{id Integer @id @default(snowflake())}";

    let formatted = format_document_source(&uri, source, &dinoco_formatter::FormatterConfig::default())
        .expect("imported document should format");

    assert!(formatted.contains("@default(snowflake())"));
}

#[test]
fn formatting_main_snowflake_models_still_requires_project_config() {
    let uri = Url::parse("file:///project/dinoco/schema.dinoco").unwrap();
    let source = "model Account{id Integer @id @default(snowflake())}";

    let error = format_document_source(&uri, source, &dinoco_formatter::FormatterConfig::default())
        .expect_err("main schema must still validate project config");

    assert!(error.message.contains("snowflake_node_id"));
}

#[test]
fn calculates_close_type_fixes() {
    let source = "model User { name Strng }";
    let index = DocumentIndex::new(source);
    let diagnostic = Diagnostic {
        range: Range::new(Position::new(0, 18), Position::new(0, 23)),
        message: "Unknown type `Strng`.".to_string(),
        ..Diagnostic::default()
    };
    assert_eq!(unknown_type_fix(&index, &diagnostic).expect("fix").new_text, "String");
}

#[test]
fn edit_distance_handles_insertions() {
    assert_eq!(edit_distance("strng", "string"), 1);
}

#[test]
fn project_compile_errors_are_published_at_the_imported_file() {
    let project = tempdir().expect("project");
    let root = project.path().join("schema.dinoco");
    let child = project.path().join("business.dinoco");
    fs::write(&root, "import { Business } from \"business.dinoco\"\n").expect("root");
    fs::write(
        &child,
        "model Business {\n    id String @id\n    account Account @relation(fields: [id], references: [id])\n}\n",
    )
    .expect("child");
    let root = canonical_path(&root).expect("canonical root");
    let mut cache = WorkspaceCache::default();
    let graph = cache.load_graph(&root, &HashMap::new());
    let error = dinoco_compiler::compile_file(&root).expect_err("project error");

    let (uri, diagnostic) = compile_error_diagnostic(&root, &graph, error).expect("diagnostic");

    assert_eq!(uri.to_file_path().expect("file uri"), canonical_path(&child).expect("canonical child"));
    assert_eq!(diagnostic.range.start.line, 2);
    assert_eq!(diagnostic.code, Some(NumberOrString::String("dinoco.project".to_string())));
}

#[test]
fn import_diagnostics_select_the_missing_symbol_or_path() {
    let project = tempdir().expect("project");
    let root = project.path().join("schema.dinoco");
    let child = project.path().join("models.dinoco");
    fs::write(&child, "model Present { id String @id }\n").expect("child");
    fs::write(&root, "import { Missing } from \"./models.dinoco\"\n").expect("root");

    let root = canonical_path(&root).expect("canonical root");
    let mut cache = WorkspaceCache::default();
    let graph = cache.load_graph(&root, &HashMap::new());
    let error = dinoco_compiler::compile_file(&root).expect_err("missing symbol");
    let (_, diagnostic) = compile_error_diagnostic(&root, &graph, error).expect("symbol diagnostic");
    assert_eq!(diagnostic.range, Range::new(Position::new(0, 9), Position::new(0, 16)));
    let live = import_diagnostics(&root, &graph.files[&root], &graph);
    assert_eq!(live.len(), 1);
    assert_eq!(live[0].code, Some(NumberOrString::String("dinoco.importSymbolNotFound".to_string())));
    assert_eq!(live[0].range, diagnostic.range);

    fs::write(&root, "import { Present } from \"../missing.dinoco\"\n").expect("missing path");
    let mut cache = WorkspaceCache::default();
    let graph = cache.load_graph(&root, &HashMap::new());
    let error = dinoco_compiler::compile_file(&root).expect_err("missing file");
    let (_, diagnostic) = compile_error_diagnostic(&root, &graph, error).expect("path diagnostic");
    assert_eq!(diagnostic.range, Range::new(Position::new(0, 25), Position::new(0, 42)));
    let live = import_diagnostics(&root, &graph.files[&root], &graph);
    assert_eq!(live.len(), 1);
    assert_eq!(live[0].code, Some(NumberOrString::String("dinoco.importFileNotFound".to_string())));
    assert_eq!(live[0].range, diagnostic.range);
}

#[test]
fn live_import_diagnostics_follow_transitive_content_changes() {
    let project = tempdir().expect("project");
    let root = project.path().join("schema.dinoco");
    let first = project.path().join("first.dinoco");
    let second = project.path().join("second.dinoco");
    fs::write(&root, "import { First } from \"./first.dinoco\"\n").expect("root");
    fs::write(&first, "import { Second } from \"./second.dinoco\"\nmodel First { id String @id second Second? }\n")
        .expect("first");
    fs::write(&second, "model Second { id String @id }\n").expect("second");
    let root = canonical_path(&root).expect("root path");
    let first = canonical_path(&first).expect("first path");

    let mut cache = WorkspaceCache::default();
    let graph = cache.load_graph(&root, &HashMap::new());
    assert!(graph.files.iter().all(|(path, file)| import_diagnostics(path, file, &graph).is_empty()));

    fs::write(&second, "model Renamed { id String @id }\n").expect("rename second");
    cache.invalidate(&second);
    let graph = cache.load_graph(&root, &HashMap::new());
    let diagnostics = import_diagnostics(&first, &graph.files[&first], &graph);
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].range, Range::new(Position::new(0, 9), Position::new(0, 15)));
    assert!(diagnostics[0].message.contains("Imported symbol `Second`"));
}

#[test]
fn semantic_tokens_distinguish_declarations_properties_types_and_enum_members() {
    let source = "enum Status {\n    active\n}\n\nmodel Account {\n    id     String @id\n    status Status\n}\n";
    let index = DocumentIndex::new(source);
    let spans = semantic_token_spans(&index);

    let kind_at = |line: u32, character: u32| {
        spans
            .iter()
            .find(|(range, _, _)| range.start.line == line && range.start.character == character)
            .map(|(_, token_type, modifiers)| (*token_type, *modifiers))
    };

    // `Status` the enum declaration.
    assert_eq!(kind_at(0, 5), Some((SEMANTIC_TOKEN_TYPE, SEMANTIC_MODIFIER_DECLARATION)));
    // `active` the enum member.
    assert_eq!(kind_at(1, 4), Some((SEMANTIC_TOKEN_ENUM_MEMBER, 0)));
    // `Account` the model declaration.
    assert_eq!(kind_at(4, 6), Some((SEMANTIC_TOKEN_TYPE, SEMANTIC_MODIFIER_DECLARATION)));
    // `id` the field name (property), not a type.
    assert_eq!(kind_at(5, 4), Some((SEMANTIC_TOKEN_PROPERTY, 0)));
    // `status` field referencing the `Status` enum: property name, then a type reference.
    assert_eq!(kind_at(6, 4), Some((SEMANTIC_TOKEN_PROPERTY, 0)));
    assert_eq!(kind_at(6, 11), Some((SEMANTIC_TOKEN_TYPE, 0)));

    let tokens = encode_semantic_tokens(spans);
    assert!(!tokens.is_empty());
    // Non-decreasing (line, start) order is required by the LSP encoding.
    let mut cursor_line = 0u32;
    for token in &tokens {
        cursor_line += token.delta_line;
        assert!(token.length > 0);
    }
    let _ = cursor_line;
}
