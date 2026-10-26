//! Runs project-owned Rust: `dinoco/transform.rs` while generating models,
//! and the project's seed for `dinoco seed`, through the application's tests.
//!
//! The CLI is a prebuilt binary and cannot load user code, so it generates a
//! tiny cargo project in `dinoco/.runner/` that `#[path]`-includes the
//! transform and runs it. It builds into the project's
//! own target directory with a copy of the project's `Cargo.lock`, so it reuses
//! the dependencies the application already compiled instead of building them
//! again. Set `DINOCO_RUNNER_CRATES_PATH` to a Dinoco checkout to build the
//! runner against local crates instead of the published ones.

use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, bail};
use dinoco_compiler::Schema;

use crate::schema::{Database, PostgresConnection, RuntimeConfig, SchemaSource};

const RUNNER_DIR: &str = "dinoco/.runner";

/// `dinoco/transform.rs`, when the project customizes generated code.
pub fn transform_path() -> &'static Path {
    Path::new(dinoco_codegen::TRANSFORM_PATH)
}

pub fn has_transform() -> bool {
    transform_path().is_file()
}

/// Generates `dinoco/models/`, applying `dinoco/transform.rs` when it exists.
///
/// With a workspace, it also saves the schema it generated from, `source`,
/// into `dinoco/migrations/<workspace>/schema/`.
pub fn generate_models(schema: &Schema, workspace: Option<&str>, source: &SchemaSource) -> anyhow::Result<()> {
    if has_transform() {
        crate::ui::progress("Applying dinoco/transform.rs (building the Dinoco runner)...");
        let schema = match source {
            SchemaSource::Current => String::new(),
            SchemaSource::Saved(from) => {
                crate::schema::workspace_schema_dir(from).join("schema.dinoco").to_string_lossy().into_owned()
            }
        };
        run(
            Task::Models,
            &[("DINOCO_RUNNER_WORKSPACE", workspace.unwrap_or("").to_string()), ("DINOCO_RUNNER_SCHEMA", schema)],
        )?;
    } else {
        dinoco_codegen::generate_models_for_workspace(schema, workspace)?;
    }

    if let Some(workspace) = workspace {
        let saved = crate::schema::save_workspace_schema(workspace, source)?;
        let files = crate::ui::plural(saved.len(), "file", "files");
        let target = crate::schema::workspace_schema_dir(workspace);
        match source {
            SchemaSource::Saved(from) if from != workspace => {
                crate::ui::info(format!("Schema of `{from}` saved to {} ({files})", target.display()))
            }
            _ => crate::ui::info(format!("Schema saved to {} ({files})", target.display())),
        }
    }

    Ok(())
}

/// Runs the project's seed against the database `config` describes.
///
/// The seed is part of the application's crate, and uses its items and
/// dependencies under `crate::`, so it cannot be included in the runner like
/// the transform. Instead, Cargo compiles the crate's library and binaries for
/// their tests and runs only the test the generated `dinoco/mod.rs` declares,
/// which calls the seed. The report file tells whether one of them ran it.
pub fn seed(config: &RuntimeConfig) -> anyhow::Result<()> {
    let database = match config.database {
        Database::Postgresql => "postgresql",
        Database::Mysql => "mysql",
        Database::Sqlite => "sqlite",
    };
    let connection = match config.postgres_connection {
        PostgresConnection::Direct => "direct",
        PostgresConnection::PgBouncer => "pgbouncer",
    };
    let runner = Path::new(RUNNER_DIR);
    fs::create_dir_all(runner)?;
    fs::write(runner.join(".gitignore"), "*\n")?;
    let report = std::path::absolute(runner.join("seed-report"))?;
    if report.exists() {
        fs::remove_file(&report)?;
    }

    let status = Command::new(cargo())
        .args(["test", "--quiet"])
        .args(test_selection()?)
        .args(["--", dinoco_codegen::SEED_TEST, "--ignored", "--nocapture", "--test-threads=1"])
        .env("DINOCO_SEED_REPORT", &report)
        .env("DINOCO_SEED_DATABASE", database)
        .env("DINOCO_SEED_CONNECTION", connection)
        .env("DINOCO_SEED_DATABASE_URL", &config.database_url)
        .status()
        .context("failed to launch `cargo`; `dinoco seed` needs a Rust toolchain")?;
    if !status.success() {
        bail!("the seed failed (see the output above)");
    }
    if !report.exists() {
        bail!(
            "no target of this crate includes dinoco/mod.rs, so the seed never ran; declare it in src/main.rs or src/lib.rs, e.g. `#[path = \"../dinoco/mod.rs\"] mod dinoco;`"
        );
    }
    fs::remove_file(report)?;

    Ok(())
}

/// Selects the package in the current directory, never the workspace's
/// default members, and its library and binaries, never its integration tests,
/// examples or doctests: `--lib` and `--bins` only for the targets it has, as
/// Cargo rejects `--lib` in a package without a library.
fn test_selection() -> anyhow::Result<Vec<String>> {
    let output = Command::new(cargo())
        .args(["metadata", "--format-version", "1", "--no-deps"])
        .output()
        .context("failed to launch `cargo`; `dinoco seed` needs a Rust toolchain")?;
    if !output.status.success() {
        bail!("`cargo metadata` failed: {}", String::from_utf8_lossy(&output.stderr).trim());
    }
    let metadata: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    let manifest = std::path::absolute("Cargo.toml")?;
    let package = metadata["packages"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|package| package["manifest_path"].as_str().is_some_and(|path| Path::new(path) == manifest))
        .context("`dinoco seed` runs from the root of the Cargo package that includes dinoco/mod.rs")?;
    let has_kind = |kind: &str| {
        package["targets"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|target| target["kind"].as_array().into_iter().flatten().any(|value| value.as_str() == Some(kind)))
    };

    let id = package["id"].as_str().context("`cargo metadata` did not identify the package")?;
    let mut selection = vec!["--package".to_string(), id.to_string()];
    if has_kind("lib") {
        selection.push("--lib".to_string());
    }
    if has_kind("bin") {
        selection.push("--bins".to_string());
    }
    if selection.len() == 2 {
        bail!("this package has no library or binary to include dinoco/mod.rs, so it has no seed to run");
    }
    Ok(selection)
}

#[derive(Clone, Copy)]
enum Task {
    Models,
}

impl Task {
    fn name(self) -> &'static str {
        match self {
            Task::Models => "models",
        }
    }
}

/// Builds and runs the runner's binary for `task`, with `envs` set.
fn run(task: Task, envs: &[(&str, String)]) -> anyhow::Result<()> {
    let id = project_id();
    let project = cargo_project();
    write_project(&id, project.as_ref())?;

    let mut command = Command::new(cargo());
    command
        .args(["run", "--quiet", "--manifest-path"])
        .arg(format!("{RUNNER_DIR}/Cargo.toml"))
        .arg("--bin")
        .arg(format!("dinoco-{}-{id}", task.name()))
        .envs(envs.iter().map(|(name, value)| (name, value)));
    if let Some(project) = &project {
        command.env("CARGO_TARGET_DIR", &project.target_directory);
    }
    let status = command.status().context("failed to launch `cargo`; the Dinoco runner needs a Rust toolchain")?;
    if !status.success() {
        bail!("the Dinoco runner failed (see the output above)");
    }

    Ok(())
}

fn cargo() -> String {
    std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string())
}

/// Identifies the project, so projects that share a target directory never
/// overwrite each other's runner binaries.
fn project_id() -> String {
    let mut hasher = DefaultHasher::new();
    std::env::current_dir().unwrap_or_default().hash(&mut hasher);
    format!("{:x}", hasher.finish())
}

/// The Cargo project the CLI runs in.
struct CargoProject {
    /// Where the project builds: `CARGO_TARGET_DIR`, `.cargo/config.toml`, or
    /// `target/` at its workspace root.
    target_directory: PathBuf,
    workspace_root: PathBuf,
}

/// Asks Cargo about the project in the current directory; `None` outside one,
/// and then the runner builds in its own directory.
fn cargo_project() -> Option<CargoProject> {
    let output = Command::new(cargo()).args(["metadata", "--format-version", "1", "--no-deps"]).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let metadata: serde_json::Value = serde_json::from_slice(&output.stdout).ok()?;
    let path = |key: &str| metadata.get(key)?.as_str().map(PathBuf::from);

    Some(CargoProject { target_directory: path("target_directory")?, workspace_root: path("workspace_root")? })
}

fn write_project(id: &str, project: Option<&CargoProject>) -> anyhow::Result<()> {
    let runner = Path::new(RUNNER_DIR);
    let src = runner.join("src");
    fs::create_dir_all(&src)?;
    fs::write(runner.join(".gitignore"), "*\n")?;
    fs::write(runner.join("Cargo.toml"), render_manifest(id))?;
    fs::write(src.join("models.rs"), MODELS_MAIN)?;
    // Left by runners of earlier releases: a single binary, and the seed's.
    for legacy in ["main.rs", "seed.rs"] {
        let legacy = src.join(legacy);
        if legacy.exists() {
            fs::remove_file(legacy)?;
        }
    }

    // The lockfile decides the dependency versions. The project's own makes
    // the runner build the exact versions the application did, so their builds
    // are reused; it is copied again only when it changes, so the runner does
    // not resolve its extra dependencies on every run. Without one, a local
    // checkout resolves offline against its own.
    let lock = runner.join("Cargo.lock");
    let project_lock = project.map(|project| project.workspace_root.join("Cargo.lock")).filter(|lock| lock.is_file());
    if let Some(source) = project_lock {
        if is_newer(&source, &lock) {
            fs::copy(source, &lock)?;
        }
    } else if let Ok(root) = std::env::var("DINOCO_RUNNER_CRATES_PATH") {
        let source = Path::new(&root).join("Cargo.lock");
        if !lock.exists() && source.is_file() {
            fs::copy(source, &lock)?;
        }
    }

    Ok(())
}

/// `source` was modified after `target`, or `target` does not exist.
fn is_newer(source: &Path, target: &Path) -> bool {
    let modified = |path: &Path| fs::metadata(path).and_then(|metadata| metadata.modified()).ok();
    match (modified(source), modified(target)) {
        (Some(source), Some(target)) => source > target,
        _ => true,
    }
}

fn render_manifest(id: &str) -> String {
    let version = env!("CARGO_PKG_VERSION");
    let dependency = |name: &str| match std::env::var("DINOCO_RUNNER_CRATES_PATH") {
        Ok(root) => format!("{name} = {{ path = \"{}/crates/{name}\" }}\n", root.trim_end_matches('/')),
        Err(_) => format!("{name} = \"={version}\"\n"),
    };

    let mut manifest = format!(
        "# Generated by the Dinoco CLI. Safe to delete; it is recreated on demand.\n\
         [package]\nname = \"dinoco-runner-{id}\"\nversion = \"0.0.0\"\nedition = \"2024\"\npublish = false\nautobins = false\n\n\
         [workspace]\n\n\
         [[bin]]\nname = \"dinoco-models-{id}\"\npath = \"src/models.rs\"\n\n\
         [dependencies]\n"
    );
    for name in ["dinoco", "dinoco_codegen", "dinoco_compiler"] {
        manifest.push_str(&dependency(name));
    }
    manifest.push_str("anyhow = \"1.0\"\n");
    manifest
}

/// Generates the models with `dinoco/transform.rs` applied. It reads the
/// schema itself and never includes the models it is about to replace.
const MODELS_MAIN: &str = r#"// Generated by the Dinoco CLI.
#![allow(unused)]

#[path = "../../transform.rs"]
mod transform;

fn main() -> ::anyhow::Result<()> {
    let workspace = std::env::var("DINOCO_RUNNER_WORKSPACE").ok().filter(|value| !value.is_empty());
    let compile = |path: &str| {
        ::dinoco_compiler::compile_file(std::path::Path::new(path)).map_err(|error| ::anyhow::anyhow!(error.to_string()))
    };
    let schema = compile("dinoco/schema.dinoco")?;
    let mut schema = match workspace.as_deref() {
        Some(name) => schema.for_workspace(name).ok_or_else(|| ::anyhow::anyhow!("workspace `{name}` was not found"))?,
        None => schema,
    };

    // Another workspace's saved schema: its models and enums, with the config
    // of dinoco/schema.dinoco.
    if let Some(path) = std::env::var("DINOCO_RUNNER_SCHEMA").ok().filter(|value| !value.is_empty()) {
        let is_config = |item: &::dinoco_compiler::SchemaItem| matches!(item, ::dinoco_compiler::SchemaItem::Config(_));
        let config = schema.items.into_iter().find(is_config);
        schema = compile(&path)?;
        schema.items.retain(|item| !is_config(item));
        schema.items.splice(0..0, config);
    }

    // The CLI reports the result.
    ::dinoco_codegen::generate_models_with(&schema, workspace.as_deref(), &transform::transformer())?;
    Ok(())
}
"#;
