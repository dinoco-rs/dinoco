//! `migration_engine = "manual"`: hand-written Rust migrations.

use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, bail};
use dinoco_compiler::Schema;
use inquire::Text;

use crate::runner::{self, Task};
use crate::schema::{read_schema_for_workspace, runtime_config};
use crate::ui;

const MODS_END: &str = "// dinoco:migrations:mods:end";
const LIST_END: &str = "// dinoco:migrations:list:end";

/// `dinoco migrate generate [name]`: scaffolds a migration file, registers it
/// and regenerates the models.
pub async fn generate(schema: &Schema, workspace: Option<&str>, name: Option<String>) -> anyhow::Result<()> {
    let name = match name {
        Some(name) => name,
        None => Text::new("Migration name?").with_help_message("e.g. create_users").prompt()?,
    };
    let name = snake_case(&name);
    if name.is_empty() {
        bail!("the migration name must contain at least one letter or digit");
    }

    let directory = dinoco_codegen::manual_migrations_dir(workspace);
    fs::create_dir_all(&directory)?;
    let mod_path = directory.join("mod.rs");
    if !mod_path.exists() {
        fs::write(&mod_path, dinoco_codegen::migrations_mod_template())?;
    }

    let timestamp = timestamp_now();
    let migration = scaffold(&mod_path, &directory, &timestamp, &name)?;
    ui::success(format!("Manual migration created: {}", migration.display()));

    runner::generate_models(schema, workspace)?;
    ui::success("Rust models generated at dinoco/models/");
    ui::info("Fill in `up` and `down`, then run `dinoco migrate run`.");

    Ok(())
}

/// Creates `<timestamp>_<name>.rs` and registers it in `mod.rs`.
pub fn scaffold(mod_path: &Path, directory: &Path, timestamp: &str, name: &str) -> anyhow::Result<std::path::PathBuf> {
    let migration_name = format!("{timestamp}_{name}");
    let module = format!("m{migration_name}");
    let file = directory.join(format!("{migration_name}.rs"));
    if file.exists() {
        bail!("{} already exists", file.display());
    }

    let mod_source = fs::read_to_string(mod_path).with_context(|| format!("failed to read {}", mod_path.display()))?;
    let struct_name = pascal_case(name);
    let mod_source =
        insert_before(&mod_source, MODS_END, &format!("#[path = \"{migration_name}.rs\"]\nmod {module};\n"))?;
    let mod_source = insert_before(
        &mod_source,
        LIST_END,
        &format!("::dinoco::MigrationEntry::new(\"{migration_name}\", {module}::{struct_name}),\n"),
    )?;

    fs::write(&file, render_migration(&struct_name))?;
    fs::write(mod_path, mod_source)?;

    Ok(file)
}

fn render_migration(struct_name: &str) -> String {
    format!(
        r#"use ::dinoco::*;

#[dinoco(migration)]
pub struct {struct_name};

impl DinocoMigration for {struct_name} {{
    async fn up(&self, manager: &DinocoManager) -> Result<(), DbErr> {{
        // manager
        //     .create_table(CreateTableMigration {{
        //         table: "example".to_string(),
        //         if_not_exists: false,
        //         columns: vec![MigrationColumn::new("id", MigrationColumnType::String).primary_key()],
        //         foreign_keys: Vec::new(),
        //     }})
        //     .await?;

        Ok(())
    }}

    async fn down(&self, manager: &DinocoManager) -> Result<(), DbErr> {{
        // manager
        //     .drop_table(DropTableMigration {{ table: "example".to_string(), if_exists: true }})
        //     .await?;

        Ok(())
    }}
}}
"#
    )
}

/// Inserts `line` on its own line right before the (indented) `marker` line,
/// keeping the marker's indentation.
fn insert_before(source: &str, marker: &str, line: &str) -> anyhow::Result<String> {
    let mut out = String::new();
    let mut found = false;
    for current in source.split_inclusive('\n') {
        if !found && current.trim() == marker {
            let indent = &current[..current.len() - current.trim_start().len()];
            for part in line.lines() {
                out.push_str(indent);
                out.push_str(part);
                out.push('\n');
            }
            found = true;
        }
        out.push_str(current);
    }

    if !found {
        bail!("dinoco/migrations/mod.rs is missing the `{marker}` marker; restore it or add the migration by hand");
    }
    Ok(out)
}

pub async fn run(workspace: Option<String>) -> anyhow::Result<()> {
    execute(workspace, |_| Task::MigrateUp).await
}

pub async fn rollback(workspace: Option<String>, steps: usize) -> anyhow::Result<()> {
    if steps == 0 {
        bail!("--steps must be at least 1");
    }
    execute(workspace, move |_| Task::MigrateDown(steps)).await
}

pub async fn status(workspace: Option<String>) -> anyhow::Result<()> {
    execute(workspace, |_| Task::MigrateStatus).await
}

async fn execute(workspace: Option<String>, task: impl FnOnce(&Schema) -> Task) -> anyhow::Result<()> {
    let (_, schema, workspace) = read_schema_for_workspace(workspace.as_deref())?;
    let config = runtime_config(&schema)?;
    let mod_path = dinoco_codegen::manual_migrations_dir(workspace.as_deref()).join("mod.rs");
    if !mod_path.is_file() {
        bail!(
            "{} was not found. Run `dinoco migrate generate <name>` to create your first manual migration.",
            mod_path.display()
        );
    }

    let task = task(&schema);
    tokio::task::spawn_blocking(move || runner::run(&task, &schema, workspace.as_deref(), Some(&config)))
        .await
        .context("the runner task panicked")?
}

fn timestamp_now() -> String {
    let seconds = SystemTime::now().duration_since(UNIX_EPOCH).map(|elapsed| elapsed.as_secs()).unwrap_or_default();
    format_timestamp(seconds)
}

/// `YYYYMMDDHHMMSS` in UTC.
pub fn format_timestamp(seconds: u64) -> String {
    let days = (seconds / 86_400) as i64;
    let remainder = seconds % 86_400;
    // Civil-from-days (Howard Hinnant).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z.rem_euclid(146_097);
    let year_of_era = (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = if month_prime < 10 { month_prime + 3 } else { month_prime - 9 };
    let year = year_of_era + era * 400 + i64::from(month <= 2);

    format!("{year:04}{month:02}{day:02}{:02}{:02}{:02}", remainder / 3_600, (remainder % 3_600) / 60, remainder % 60)
}

pub fn snake_case(value: &str) -> String {
    let mut out = String::new();
    let mut previous_lower = false;
    for character in value.trim().chars() {
        if character.is_ascii_alphanumeric() {
            if character.is_ascii_uppercase() && previous_lower {
                out.push('_');
            }
            out.push(character.to_ascii_lowercase());
            previous_lower = character.is_ascii_lowercase() || character.is_ascii_digit();
        } else {
            if !out.ends_with('_') && !out.is_empty() {
                out.push('_');
            }
            previous_lower = false;
        }
    }
    out.trim_matches('_').to_string()
}

pub fn pascal_case(snake: &str) -> String {
    let name = snake
        .split('_')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut characters = part.chars();
            characters
                .next()
                .map(|first| first.to_ascii_uppercase().to_string() + characters.as_str())
                .unwrap_or_default()
        })
        .collect::<String>();

    if name.chars().next().is_some_and(|first| first.is_ascii_digit()) { format!("Migration{name}") } else { name }
}
