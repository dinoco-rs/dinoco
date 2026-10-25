use std::{fs, path::Path};

// The documentation site lives outside this repository (it covers every crate of the
// Dinoco ecosystem), so its versioned directories are checked by its own scripts.
const CRATE_VERSION: &str = "2.2.1";
const PREVIOUS_CRATE_VERSION: &str = "2.2.0";

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
        "vscode/Cargo.toml",
    ];

    // The version lives once, in the root manifest: `[workspace.package]` and
    // the internal dependencies under `[workspace.dependencies]`.
    let root = read("Cargo.toml");
    assert!(
        root.contains(&format!("[workspace.package]\nversion = \"{CRATE_VERSION}\"")),
        "Cargo.toml does not declare the workspace version {CRATE_VERSION}"
    );
    for (crate_name, path) in [
        ("dinoco", "crates/dinoco"),
        ("dinoco_cli", "crates/dinoco_cli"),
        ("dinoco_codegen", "crates/dinoco_codegen"),
        ("dinoco_compiler", "crates/dinoco_compiler"),
        ("dinoco_engine", "crates/dinoco_engine"),
        ("dinoco_formatter", "crates/dinoco_formatter"),
        ("dinoco_vscode", "vscode"),
    ] {
        assert!(
            root.contains(&format!("{crate_name} = {{ path = \"{path}\", version = \"{CRATE_VERSION}\"")),
            "Cargo.toml does not depend on {crate_name} {CRATE_VERSION}"
        );
    }
    assert!(
        root.contains(&format!(
            "dinoco_derives = {{ path = \"crates/dinoco_derives\", version = \"={CRATE_VERSION}\" }}"
        )),
        "Cargo.toml does not pin dinoco_derives to exactly {CRATE_VERSION}"
    );
    assert!(!root.contains(PREVIOUS_CRATE_VERSION), "Cargo.toml still references {PREVIOUS_CRATE_VERSION}");

    for manifest_path in cargo_manifests {
        let manifest = read(manifest_path);
        assert!(
            manifest.contains("version.workspace = true"),
            "{manifest_path} does not inherit the workspace version"
        );
        assert!(
            !manifest.lines().any(|line| line.starts_with("version = ")),
            "{manifest_path} declares a version of its own instead of inheriting it"
        );
    }

    let vscode_package_json = read("vscode/package.json");
    assert!(
        vscode_package_json.contains(&format!("\"version\": \"{CRATE_VERSION}\"")),
        "vscode/package.json does not declare version {CRATE_VERSION}"
    );
    assert!(!vscode_package_json.contains(PREVIOUS_CRATE_VERSION));

    let lockfile = read("Cargo.lock");
    for package_name in [
        "dinoco",
        "dinoco_cli",
        "dinoco_codegen",
        "dinoco_compiler",
        "dinoco_derives",
        "dinoco_engine",
        "dinoco_formatter",
        "dinoco_vscode",
    ] {
        let block = package_block(&lockfile, package_name);
        assert!(
            block.contains(&format!("version = \"{CRATE_VERSION}\"")),
            "Cargo.lock contains an outdated {package_name} package"
        );
    }

    for documentation_path in ["readme.md", "crates.md"] {
        let documentation = read(documentation_path);
        assert!(
            documentation.contains(&format!("dinoco = \"{CRATE_VERSION}\"")),
            "{documentation_path} does not show the current Dinoco dependency"
        );
        assert!(!documentation.contains(PREVIOUS_CRATE_VERSION));
    }
}
