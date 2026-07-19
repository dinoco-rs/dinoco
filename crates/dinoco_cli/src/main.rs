#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dinoco_cli::run().await
}
