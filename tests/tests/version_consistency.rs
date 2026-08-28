use std::{fs, path::Path};

const VERSION: &str = "1.2.6";
const PREVIOUS_VERSION: &str = "1.2.5";

fn workspace_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().expect("tests crate must be inside the workspace")
}

fn read(relative_path: &str) -> String {
    let path = workspace_root().join(relative_path);
    fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!("failed to read {}: {error}", path.display());
    })
}

fn package_block<'a>(lockfile: &'a str, package_name: &str) -> &'a str {
    let marker = format!("[[package]]\nname = \"{package_name}\"\n");
    let start = lockfile.find(&marker).unwrap_or_else(|| panic!("package {package_name} is missing from Cargo.lock"));
    let tail = &lockfile[start..];
    let end = tail[marker.len()..].find("\n[[package]]").map(|offset| marker.len() + offset).unwrap_or(tail.len());

    &tail[..end]
}

#[test]
fn release_version_is_consistent_across_the_workspace() {
    let cargo_manifests = [
        "crates/dinoco/Cargo.toml",
        "crates/dinoco_cli/Cargo.toml",
        "crates/dinoco_codegen/Cargo.toml",
        "crates/dinoco_compiler/Cargo.toml",
        "crates/dinoco_derives/Cargo.toml",
        "crates/dinoco_engine/Cargo.toml",
        "crates/dinoco_formatter/Cargo.toml",
        "examples/actix-web/Cargo.toml",
        "examples/axum/Cargo.toml",
        "tests/Cargo.toml",
        "vscode/Cargo.toml",
    ];

    for manifest_path in cargo_manifests {
        let manifest = read(manifest_path);
        assert!(
            manifest.contains(&format!("version = \"{VERSION}\"")),
            "{manifest_path} does not declare version {VERSION}"
        );
        assert!(!manifest.contains(PREVIOUS_VERSION), "{manifest_path} still references {PREVIOUS_VERSION}");
    }

    let lockfile = read("Cargo.lock");
    for package_name in [
        "dinoco",
        "dinoco-example-actix-web",
        "dinoco-example-axum",
        "dinoco_cli",
        "dinoco_codegen",
        "dinoco_compiler",
        "dinoco_derives",
        "dinoco_engine",
        "dinoco_formatter",
        "dinoco_tests",
        "dinoco_vscode",
    ] {
        let block = package_block(&lockfile, package_name);
        assert!(
            block.contains(&format!("version = \"{VERSION}\"")),
            "Cargo.lock contains an outdated {package_name} package"
        );
    }

    for documentation_path in ["readme.md", "crates.md"] {
        let documentation = read(documentation_path);
        assert!(
            documentation.contains(&format!("dinoco = \"{VERSION}\"")),
            "{documentation_path} does not show the current Dinoco dependency"
        );
        assert!(!documentation.contains(PREVIOUS_VERSION));
    }

    for package_path in ["docs/package.json", "vscode/package.json", "vscode/package-lock.json"] {
        let package = read(package_path);
        assert!(package.contains(&format!("\"version\": \"{VERSION}\"")));
    }
}

#[test]
fn documentation_uses_the_current_version_directories() {
    let content_dir = workspace_root().join(format!("docs/src/content/v{VERSION}"));
    let navigation_dir = workspace_root().join(format!("docs/src/jsons/versions/v{VERSION}"));
    let previous_content_dir = workspace_root().join(format!("docs/src/content/v{PREVIOUS_VERSION}"));
    let previous_navigation_dir = workspace_root().join(format!("docs/src/jsons/versions/v{PREVIOUS_VERSION}"));

    assert!(content_dir.is_dir());
    assert!(navigation_dir.is_dir());
    assert!(!previous_content_dir.exists());
    assert!(!previous_navigation_dir.exists());

    let versions = read("docs/src/jsons/versions.ts");
    assert!(versions.contains("import v1_2_6 from './versions/v1.2.6';"));
    assert!(versions.contains("const versionsData: DocsVersionData[] = [v1_2_6"));
    assert!(!versions.contains("v1_2_1"));
    assert!(!versions.contains("v1.2.1"));

    for locale in ["en-us", "pt-br"] {
        let release_notes = read(&format!("docs/src/content/v{VERSION}/{locale}/release-notes.md"));
        assert!(release_notes.starts_with(&format!("# Dinoco v{VERSION}\n")));
    }
}
