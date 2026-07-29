use dinoco_vscode::completion::complete;
use dinoco_vscode::diagnostics::analyze;
use dinoco_vscode::document::DocumentIndex;
use dinoco_vscode::tower_lsp::lsp_types::{CompletionResponse, NumberOrString, Position};

#[test]
fn schema_completion_exposes_standard_and_fulltext_indexes() {
    let source = "model Account {\n    biography String @\n}";
    let index = DocumentIndex::new(source);
    let CompletionResponse::Array(items) = complete(source, &index, Position::new(1, 22)) else {
        panic!("completion should return an item array");
    };

    assert!(items.iter().any(|item| item.label == "@index"));
    assert!(items.iter().any(|item| item.label == "@fulltext"));
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
