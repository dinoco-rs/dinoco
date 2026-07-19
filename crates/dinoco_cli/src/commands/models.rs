use crate::schema::read_schema;
use crate::ui;

pub async fn generate() -> anyhow::Result<()> {
    let (_, schema) = read_schema()?;
    dinoco_codegen::generate_models(&schema)?;
    ui::success("Rust models generated at dinoco/models/");
    Ok(())
}
