use std::path::Path;

use crate::error::CliError;
use crate::schema::{read_schema_for_workspace, runtime_config};
use crate::ui;

const WORKSPACE_QUESTION: &str = "Which workspace do you want to seed?";

/// `dinoco seed`: runs the project's seed against the database of the
/// workspace.
pub async fn run(workspace: Option<String>) -> anyhow::Result<()> {
    let [file, directory] = dinoco_codegen::SEED_PATHS;
    let seed = match (Path::new(file).is_file(), Path::new(directory).is_file()) {
        (true, true) => {
            return Err(CliError::usage("The seed is defined twice.")
                .reason(format!("Both {file} and {directory} exist, and Rust accepts only one of them."))
                .hint(format!("Keep one: move {file} into {directory}, or delete the directory."))
                .into());
        }
        (true, false) => file,
        (false, true) => directory,
        (false, false) => {
            return Err(CliError::usage("There is no seed to run.")
                .reason(format!("Neither {file} nor {directory} exists."))
                .hint(format!(
                    "Create {file} with `pub async fn seed(client: &dinoco::DinocoClient) -> dinoco::anyhow::Result<()>`, then run `dinoco models generate`."
                ))
                .into());
        }
    };
    let declared = std::fs::read_to_string("dinoco/mod.rs").is_ok_and(|module| module.contains("pub mod seed;"));
    if !declared {
        return Err(CliError::usage("dinoco/mod.rs does not declare the seed yet.")
            .reason(format!("The seed in {seed} runs through dinoco/mod.rs, which declares it once regenerated."))
            .hint("Run `dinoco models generate`, then `dinoco seed` again.")
            .into());
    }

    let (_, schema, workspace) = read_schema_for_workspace(workspace.as_deref(), WORKSPACE_QUESTION)?;
    let config = runtime_config(&schema)?;
    ui::heading("Target");
    let mut fields = Vec::new();
    if let Some(workspace) = &workspace {
        fields.push(("Workspace", workspace.clone()));
    }
    fields.push(("Database", config.database_label().to_string()));
    fields.push(("Location", crate::db::redact_database_url(&config.database_url)));
    fields.push(("Seed", seed.to_string()));
    ui::fields(&fields);
    ui::blank();

    ui::progress(format!("Running {seed} (building the Dinoco runner)..."));
    crate::runner::seed(&config)?;
    ui::success("Seed applied");
    Ok(())
}
