use crate::schema::{Questions, select_schema};
use crate::ui;

const QUESTIONS: Questions = Questions {
    workspace: "Which workspace do you want to generate the models for?",
    schema: Some("Which schema do you want to generate the models from?"),
};

pub async fn generate(workspace: Option<String>) -> anyhow::Result<()> {
    generate_with(workspace, None).await
}

/// `dinoco models generate`, optionally from the schema saved for the
/// `schema_from` workspace.
pub async fn generate_with(workspace: Option<String>, schema_from: Option<String>) -> anyhow::Result<()> {
    let selected = select_schema(workspace.as_deref(), schema_from.as_deref(), QUESTIONS)?;
    let workspace = selected.workspace.as_deref();
    if let Some(workspace) = workspace {
        ui::heading("Target");
        ui::fields(&[("Workspace", workspace.to_string()), ("Schema", selected.source.label())]);
        ui::blank();
    }
    crate::runner::generate_models(&selected.schema, workspace, &selected.source)?;
    ui::success("Rust models generated at dinoco/models/");
    Ok(())
}
