use std::collections::HashMap;
use std::fs;
use std::sync::Arc;

use dinoco_vscode::workspace::{WorkspaceCache, canonical_path, import_path_suggestions};
use tempfile::tempdir;

#[test]
fn circular_graph_loads_every_file_once_and_exposes_direct_symbols() {
    let project = tempdir().expect("project");
    let account = project.path().join("account.dinoco");
    let session = project.path().join("session.dinoco");
    fs::write(&account, "import { Session } from \"session.dinoco\"\nmodel Account { id String @id }")
        .expect("account");
    fs::write(&session, "import { Account } from \"account.dinoco\"\nmodel Session { id String @id }")
        .expect("session");

    let mut cache = WorkspaceCache::default();
    let graph = cache.load_graph(&account, &HashMap::new());
    let account = canonical_path(&account).expect("canonical account");
    let cached_account = graph.files[&account].clone();

    assert_eq!(graph.files.len(), 2);
    assert_eq!(graph.visible_declarations(&account).len(), 1);
    assert_eq!(graph.visible_declarations(&account)[0].name.as_ref().expect("name").name, "Session");

    let second_graph = cache.load_graph(&account, &HashMap::new());
    assert!(Arc::ptr_eq(&cached_account, &second_graph.files[&account]));
}

#[test]
fn suggests_only_schema_files_and_directories_for_import_paths() {
    let project = tempdir().expect("project");
    let current = project.path().join("schema.dinoco");
    fs::write(&current, "").expect("schema");
    fs::write(project.path().join("account.dinoco"), "model Account { id String @id }").expect("account");
    fs::write(project.path().join("notes.txt"), "ignored").expect("notes");
    fs::create_dir(project.path().join("entities")).expect("entities");

    let suggestions = import_path_suggestions(&current, "");

    assert!(suggestions.iter().any(|item| item.path == "./account.dinoco"));
    assert!(suggestions.iter().any(|item| item.path == "./entities/"));
    assert!(!suggestions.iter().any(|item| item.path.ends_with("notes.txt")));
    assert!(!suggestions.iter().any(|item| item.path == "./schema.dinoco"));
}

#[test]
fn import_target_symbols_are_available_only_for_a_valid_dinoco_path() {
    let project = tempdir().expect("project");
    let current = project.path().join("schema.dinoco");
    let target = project.path().join("entities.dinoco");
    fs::write(&current, "").expect("schema");
    fs::write(&target, "model Account { id String @id }\nenum Role { ADMIN }").expect("entities");
    let mut cache = WorkspaceCache::default();

    let (_, file) =
        cache.load_import_target(&current, "./entities.dinoco", &HashMap::new()).expect("valid import target");

    assert!(file.index.model("Account").is_some());
    assert!(file.index.enum_("Role").is_some());
    assert!(cache.load_import_target(&current, "./missing.dinoco", &HashMap::new()).is_none());
    assert!(cache.load_import_target(&current, "./entities.txt", &HashMap::new()).is_none());
}
