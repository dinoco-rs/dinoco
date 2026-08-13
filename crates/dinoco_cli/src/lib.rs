pub mod commands;
pub mod db;
pub mod schema;
pub mod sql;
pub mod ui;

use clap::{Args, Parser, Subcommand};

#[derive(Parser)]
#[command(name = "dinoco", version, disable_version_flag = true)]
#[command(about = "The Dinoco database toolkit")]
struct Cli {
    #[arg(short = 'v', long = "version", action = clap::ArgAction::Version, required = false, help = "Print version")]
    _version: Option<bool>,

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
    Generate(WorkspaceArgs),
    #[command(about = "Apply all pending migrations")]
    Run(WorkspaceArgs),
}

#[derive(Subcommand)]
enum ModelsCommands {
    #[command(about = "Generate Rust models without creating a migration")]
    Generate(WorkspaceArgs),
}

#[derive(Args)]
struct WorkspaceArgs {
    #[arg(short = 'w', long, value_name = "NAME", help = "Use the named workspace from schema.dinoco")]
    workspace: Option<String>,
}

pub async fn run() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    let cli = Cli::parse();

    match cli.command {
        Commands::Init => commands::init::run()?,
        Commands::Migrate(MigrateCommands::Generate(args)) => commands::migrate::generate(args.workspace).await?,
        Commands::Migrate(MigrateCommands::Run(args)) => commands::migrate::run(args.workspace).await?,
        Commands::Models(ModelsCommands::Generate(args)) => commands::models::generate(args.workspace).await?,
    }

    Ok(())
}
