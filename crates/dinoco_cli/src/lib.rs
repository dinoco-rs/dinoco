pub mod commands;
pub mod db;
pub mod runner;
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
    Init(InitArgs),
    #[command(about = "Generate and run database migrations")]
    #[command(subcommand)]
    Migrate(MigrateCommands),
    #[command(about = "Generate Rust models from the schema")]
    #[command(subcommand)]
    Models(ModelsCommands),
}

#[derive(Subcommand)]
enum MigrateCommands {
    #[command(
        about = "Automatic engine: diff the schema, create a migration, apply it and generate models. Manual engine: scaffold a Rust migration"
    )]
    Generate(MigrateGenerateArgs),
    #[command(about = "Apply all pending migrations")]
    Run(WorkspaceArgs),
    #[command(about = "Revert applied migrations (manual engine only)")]
    Rollback(RollbackArgs),
    #[command(about = "List applied and pending migrations (manual engine only)")]
    Status(WorkspaceArgs),
}

#[derive(Args)]
struct InitArgs {
    #[arg(long, value_name = "ENGINE", value_parser = ["automatic", "manual"], help = "How migrations are produced (default: automatic)")]
    migration_engine: Option<String>,
}

#[derive(Args)]
struct MigrateGenerateArgs {
    #[arg(value_name = "NAME", help = "Migration name (manual engine)")]
    name: Option<String>,

    #[command(flatten)]
    workspace: WorkspaceArgs,
}

#[derive(Args)]
struct RollbackArgs {
    #[arg(long, default_value_t = 1, value_name = "N", help = "How many applied migrations to revert")]
    steps: usize,

    #[command(flatten)]
    workspace: WorkspaceArgs,
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
        Commands::Init(args) => commands::init::run(args.migration_engine.as_deref())?,
        Commands::Migrate(MigrateCommands::Generate(args)) => {
            commands::migrate::generate(args.workspace.workspace, args.name).await?
        }
        Commands::Migrate(MigrateCommands::Run(args)) => commands::migrate::run(args.workspace).await?,
        Commands::Migrate(MigrateCommands::Rollback(args)) => {
            commands::migrate::rollback(args.workspace.workspace, args.steps).await?
        }
        Commands::Migrate(MigrateCommands::Status(args)) => commands::migrate::status(args.workspace).await?,
        Commands::Models(ModelsCommands::Generate(args)) => commands::models::generate(args.workspace).await?,
    }

    Ok(())
}
