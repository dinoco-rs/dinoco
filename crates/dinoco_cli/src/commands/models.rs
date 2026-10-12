use crate::schema::read_schema_for_workspace;
use crate::ui;

pub async fn generate(workspace: Option<String>) -> anyhow::Result<()> {
    let (_, schema, workspace) = read_schema_for_workspace(workspace.as_deref())?;
    if let Some(workspace) = &workspace {
        ui::heading("Target");
        ui::fields(&[("Workspace", workspace.as_str())]);
        ui::blank();
    }
    crate::runner::generate_models(&schema, workspace.as_deref())?;
    ui::success("Rust models generated at dinoco/models/");
    Ok(())
}
