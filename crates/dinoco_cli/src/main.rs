#[tokio::main]
async fn main() -> std::process::ExitCode {
    if let Err(error) = dinoco_cli::run().await {
        dinoco_cli::ui::error(format!("{error:#}"));
        return std::process::ExitCode::FAILURE;
    }

    std::process::ExitCode::SUCCESS
}
