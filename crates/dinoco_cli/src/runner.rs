//! Runs project-owned Rust (`dinoco/transform.rs` and manual migrations).
//!
//! The CLI is a prebuilt binary and cannot load user code, so it generates a
//! tiny cargo project in `dinoco/.runner/` that `#[path]`-includes those files
//! and runs it. Set `DINOCO_RUNNER_CRATES_PATH` to a Dinoco checkout to build
//! the runner against local crates instead of the published ones.

use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::Path;
use std::process::Command;

use anyhow::{Context, bail};
use dinoco_compiler::{MigrationEngine, Schema};

use crate::schema::{Database, PostgresConnection, RuntimeConfig};

const RUNNER_DIR: &str = "dinoco/.runner";

/// `dinoco/transform.rs`, when the project customizes generated code.
pub fn transform_path() -> &'static Path {
    Path::new("dinoco/transform.rs")
}

pub fn has_transform() -> bool {
    transform_path().is_file()
}

/// Generates `dinoco/models/`, applying `dinoco/transform.rs` when it exists.
///
/// With a workspace, it also saves the schema it generated from into
/// `dinoco/migrations/<workspace>/schema/`.
pub fn generate_models(schema: &Schema, workspace: Option<&str>) -> anyhow::Result<()> {
    if has_transform() {
        crate::ui::info("Applying dinoco/transform.rs (building the Dinoco runner)");
        run(&Task::Models, schema, workspace, None)?;
    } else {
        dinoco_codegen::generate_models_for_workspace(schema, workspace)?;
    }

    if let Some(workspace) = workspace {
        let saved = crate::schema::save_workspace_schema(workspace)?;
        crate::ui::info(format!(
            "Schema saved to {} ({} file{})",
            crate::schema::workspace_schema_dir(workspace).display(),
            saved.len(),
            if saved.len() == 1 { "" } else { "s" }
        ));
    }

    Ok(())
}

pub enum Task {
    Models,
    MigrateUp,
    MigrateDown(usize),
    MigrateStatus,
}

impl Task {
    fn arguments(&self) -> Vec<String> {
        match self {
            Task::Models => vec!["models".into()],
            Task::MigrateUp => vec!["up".into()],
            Task::MigrateDown(steps) => vec!["down".into(), steps.to_string()],
            Task::MigrateStatus => vec!["status".into()],
        }
    }
}

/// Builds and runs the runner for `task`. `database` is required by the
/// migration tasks.
pub fn run(
    task: &Task,
    schema: &Schema,
    workspace: Option<&str>,
    database: Option<&RuntimeConfig>,
) -> anyhow::Result<()> {
    let manual = schema.migration_engine() == MigrationEngine::Manual;
    write_project(manual, workspace)?;

    let mut command = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string()));
    command
        .args(["run", "--quiet", "--manifest-path"])
        .arg(format!("{RUNNER_DIR}/Cargo.toml"))
        .arg("--")
        .args(task.arguments())
        .env("DINOCO_RUNNER_WORKSPACE", workspace.unwrap_or(""));

    if let Some(config) = database {
        command
            .env(
                "DINOCO_RUNNER_DATABASE",
                match config.database {
                    Database::Postgresql => "postgresql",
                    Database::Mysql => "mysql",
                    Database::Sqlite => "sqlite",
                },
            )
            .env(
                "DINOCO_RUNNER_CONNECTION",
                match config.postgres_connection {
                    PostgresConnection::Direct => "direct",
                    PostgresConnection::PgBouncer => "pgbouncer",
                },
            )
            .env("DINOCO_RUNNER_DATABASE_URL", &config.database_url)
            .env("DINOCO_RUNNER_MIN_CONNECTION", config.min_connection.to_string())
            .env("DINOCO_RUNNER_MAX_CONNECTION", config.max_connection.to_string());
    }

    let status = command.status().context(
        "failed to launch `cargo`; Rust code in dinoco/ (transform.rs, manual migrations) needs a Rust toolchain",
    )?;
    if !status.success() {
        bail!("the Dinoco runner failed (see the output above)");
    }

    Ok(())
}

fn write_project(manual: bool, workspace: Option<&str>) -> anyhow::Result<()> {
    let src = Path::new(RUNNER_DIR).join("src");
    fs::create_dir_all(&src)?;
    fs::write(Path::new(RUNNER_DIR).join(".gitignore"), "*\n")?;
    fs::write(Path::new(RUNNER_DIR).join("Cargo.toml"), render_manifest())?;
    fs::write(src.join("main.rs"), render_main(has_transform(), manual, workspace))?;

    // A local checkout resolves offline against the checkout's own lockfile.
    if let Ok(root) = std::env::var("DINOCO_RUNNER_CRATES_PATH") {
        let lock = Path::new(RUNNER_DIR).join("Cargo.lock");
        let source = Path::new(&root).join("Cargo.lock");
        if !lock.exists() && source.is_file() {
            fs::copy(source, lock)?;
        }
    }

    Ok(())
}

fn render_manifest() -> String {
    let version = env!("CARGO_PKG_VERSION");
    let dependency = |name: &str| match std::env::var("DINOCO_RUNNER_CRATES_PATH") {
        Ok(root) => format!("{name} = {{ path = \"{}/crates/{name}\" }}\n", root.trim_end_matches('/')),
        Err(_) => format!("{name} = \"={version}\"\n"),
    };

    // Projects that share a `CARGO_TARGET_DIR` must not overwrite each other's
    // runner binary, so the package name is unique per project directory.
    let mut hasher = DefaultHasher::new();
    std::env::current_dir().unwrap_or_default().hash(&mut hasher);
    let mut manifest = format!(
        "# Generated by the Dinoco CLI. Safe to delete; it is recreated on demand.\n[package]\nname = \"dinoco-runner-{:x}\"\nversion = \"0.0.0\"\nedition = \"2024\"\npublish = false\n\n[workspace]\n\n[dependencies]\n",
        hasher.finish()
    );
    for name in ["dinoco", "dinoco_codegen", "dinoco_compiler"] {
        manifest.push_str(&dependency(name));
    }
    manifest
        .push_str("anyhow = \"1.0\"\ntokio = { version = \"1.50\", features = [\"rt-multi-thread\", \"macros\"] }\n");
    manifest
}

fn render_main(transform: bool, manual: bool, workspace: Option<&str>) -> String {
    let mut out = String::from("// Generated by the Dinoco CLI.\n#![allow(unused)]\n\n");
    if transform {
        out.push_str("#[path = \"../../transform.rs\"]\nmod transform;\n\n");
    }
    if manual {
        let migrations = match workspace {
            Some(workspace) => format!("../../migrations/{workspace}/mod.rs"),
            None => "../../migrations/mod.rs".to_string(),
        };
        out.push_str(&format!("#[path = \"{migrations}\"]\nmod migrations;\n\n"));
    }

    out.push_str(
        r#"fn main() -> ::anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let task = args.next().unwrap_or_default();
    let workspace = std::env::var("DINOCO_RUNNER_WORKSPACE").ok().filter(|value| !value.is_empty());

    match task.as_str() {
"#,
    );
    if transform {
        out.push_str("        \"models\" => models(workspace.as_deref()),\n");
    }
    if manual {
        out.push_str(
            r#"        "up" | "down" | "status" => {
            let steps = args.next().and_then(|value| value.parse::<usize>().ok()).unwrap_or(1);
            ::tokio::runtime::Builder::new_multi_thread().enable_all().build()?.block_on(migrate(&task, steps))
        }
"#,
        );
    }
    out.push_str("        other => ::anyhow::bail!(\"unknown runner task `{other}`\"),\n    }\n}\n");

    if transform {
        out.push_str(
            r#"
fn models(workspace: Option<&str>) -> ::anyhow::Result<()> {
    let schema = ::dinoco_compiler::compile_file(std::path::Path::new("dinoco/schema.dinoco"))
        .map_err(|error| ::anyhow::anyhow!(error.to_string()))?;
    let schema = match workspace {
        Some(name) => schema.for_workspace(name).ok_or_else(|| ::anyhow::anyhow!("workspace `{name}` was not found"))?,
        None => schema,
    };

    ::dinoco_codegen::generate_models_with(&schema, workspace, &transform::transformer())?;
    println!("Rust models generated at dinoco/models/ (dinoco/transform.rs applied)");
    Ok(())
}
"#,
        );
    }
    if manual {
        out.push_str(
            r#"
async fn connect() -> ::anyhow::Result<::dinoco::DinocoClient> {
    let env = |key: &str| std::env::var(key).map_err(|_| ::anyhow::anyhow!("missing runner environment `{key}`"));
    let url = env("DINOCO_RUNNER_DATABASE_URL")?;
    let min = env("DINOCO_RUNNER_MIN_CONNECTION")?.parse::<usize>()?;
    let max = env("DINOCO_RUNNER_MAX_CONNECTION")?.parse::<usize>()?;
    let backend = match (env("DINOCO_RUNNER_DATABASE")?.as_str(), env("DINOCO_RUNNER_CONNECTION")?.as_str()) {
        ("sqlite", _) => ::dinoco::Backend::Sqlite(
            <::dinoco::SqliteAdapter as ::dinoco::DinocoAdapter>::new(url).await.map_err(::anyhow::Error::msg)?,
        ),
        ("mysql", _) => ::dinoco::Backend::Mysql(::dinoco::MySqlAdapter::new(url)),
        (_, "pgbouncer") => ::dinoco::Backend::PgBouncer(::dinoco::PgBouncerAdapter::new(url).await?),
        _ => ::dinoco::Backend::Postgres(::dinoco::PostgresAdapter::direct_with_pool(url, min, max).await?),
    };

    Ok(::dinoco::DinocoClient::new(backend))
}

async fn migrate(task: &str, steps: usize) -> ::anyhow::Result<()> {
    let client = connect().await?;
    let entries = migrations::migrations();

    match task {
        "up" => {
            let ran = ::dinoco::migrate_up(&client, &entries).await?;
            for name in &ran {
                println!("Migration applied: {name}");
            }
            if ran.is_empty() {
                println!("No pending migrations.");
            } else {
                println!("Applied {} migration(s).", ran.len());
            }
        }
        "down" => {
            let reverted = ::dinoco::migrate_down(&client, &entries, steps).await?;
            for name in &reverted {
                println!("Migration reverted: {name}");
            }
            if reverted.is_empty() {
                println!("No applied migrations to revert.");
            }
        }
        _ => {
            let status = ::dinoco::migration_status(&client, &entries).await?;
            if status.is_empty() {
                println!("No manual migrations were found.");
            }
            for item in status {
                println!("[{}] {}", if item.applied { "applied" } else { "pending" }, item.name);
            }
        }
    }

    Ok(())
}
"#,
        );
    }

    out
}
