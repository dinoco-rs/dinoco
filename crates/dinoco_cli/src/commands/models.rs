use crate::schema::read_schema_for_workspace;
use crate::ui;

pub async fn generate(workspace: Option<String>) -> anyhow::Result<()> {
    let (_, schema, workspace) = read_schema_for_workspace(workspace.as_deref())?;
    dinoco_codegen::generate_models_for_workspace(&schema, workspace.as_deref())?;
    ui::success("Rust models generated at dinoco/models/");
    Ok(())
}
