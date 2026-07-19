pub mod commands;
pub mod db;
pub mod schema;
pub mod sql;
pub mod ui;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "dinoco")]
#[command(about = "The Dinoco database toolkit")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    #[command(about = "Create a dinoco/schema.dinoco file")]
    Init,
    #[command(about = "Generate and run database migrations")]
    #[command(subcommand)]
    Migrate(MigrateCommands),
    #[command(about = "Generate Rust models from the schema")]
    #[command(subcommand)]
    Models(ModelsCommands),
}

#[derive(Subcommand)]
enum MigrateCommands {
    #[command(about = "Compile the schema, create a migration, apply it, and generate models")]
    Generate,
    #[command(about = "Apply all pending migrations")]
    Run,
}

#[derive(Subcommand)]
enum ModelsCommands {
    #[command(about = "Generate Rust models without creating a migration")]
    Generate,
}

pub async fn run() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    let cli = Cli::parse();

    match cli.command {
        Commands::Init => commands::init::run()?,
        Commands::Migrate(MigrateCommands::Generate) => commands::migrate::generate().await?,
        Commands::Migrate(MigrateCommands::Run) => commands::migrate::run().await?,
        Commands::Models(ModelsCommands::Generate) => commands::models::generate().await?,
    }

    Ok(())
}
