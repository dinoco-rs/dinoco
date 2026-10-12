pub mod commands;
pub mod db;
pub mod error;
pub mod prompt;
pub mod runner;
pub mod schema;
pub mod sql;
pub mod ui;

use std::process::ExitCode;

use clap::{Args, Parser, Subcommand, ValueEnum};

use crate::commands::init::InitOptions;
use crate::commands::migrate::GenerateOptions;

#[derive(Parser)]
#[command(name = "dinoco", bin_name = "dinoco", version, disable_version_flag = true)]
#[command(about = "Schema, migration and code generation toolkit for the Dinoco ORM")]
#[command(after_help = "Run a command with --help for its options. Documentation: https://dinoco.io")]
struct Cli {
    #[arg(short = 'v', long = "version", action = clap::ArgAction::Version, required = false, help = "Print the version")]
    _version: Option<bool>,

    /// Never prompt. Commands that need a decision fail with exit code 2 and
    /// name the option that provides it. Implied when no terminal is attached
    /// or the CI environment variable is set.
    #[arg(long, global = true)]
    non_interactive: bool,

    /// Disable colored output. NO_COLOR is honored as well.
    #[arg(long, global = true)]
    no_color: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Create dinoco/schema.dinoco and dinoco/migrations/ in the current directory
    #[command(after_help = "Example:\n  dinoco init --database postgresql --connection direct")]
    Init(InitArgs),

    /// Plan, generate and apply database migrations
    #[command(subcommand)]
    Migrate(MigrateCommands),

    /// Generate the Rust models from the schema
    #[command(subcommand)]
    Models(ModelsCommands),
}

#[derive(Args)]
struct InitArgs {
    /// Database the project uses
    #[arg(long, value_enum)]
    database: Option<DatabaseArg>,

    /// How to connect to PostgreSQL
    #[arg(long, value_enum)]
    connection: Option<ConnectionArg>,
}

#[derive(Clone, Copy, ValueEnum)]
enum DatabaseArg {
    Postgresql,
    Mysql,
    Sqlite,
}

#[derive(Clone, Copy, ValueEnum)]
enum ConnectionArg {
    Direct,
    Pgbouncer,
}

#[derive(Subcommand)]
enum MigrateCommands {
    /// Compare the schema with the database, write a migration, apply it and
    /// regenerate the Rust models
    #[command(after_help = MIGRATE_GENERATE_EXAMPLES)]
    Generate(GenerateArgs),

    /// Apply every migration in dinoco/migrations/ that the database has not
    /// recorded yet
    #[command(after_help = "Example:\n  dinoco migrate run --workspace production")]
    Run(WorkspaceArgs),
}

const MIGRATE_GENERATE_EXAMPLES: &str = "Examples:
  dinoco migrate generate
  dinoco migrate generate --workspace dev --yes
  dinoco migrate generate --create-only --yes
  dinoco migrate generate --non-interactive --yes --rename user.username=display_name
  dinoco migrate generate --replace-enum-value Status.Processing=Pending --allow-destructive";

#[derive(Subcommand)]
enum ModelsCommands {
    /// Regenerate dinoco/mod.rs and dinoco/models/ without touching the database
    Generate(WorkspaceArgs),
}

#[derive(Args)]
struct WorkspaceArgs {
    /// Workspace defined in schema.dinoco. Optional when only one is
    /// defined; otherwise prompted for, or required without a terminal
    #[arg(short = 'w', long, value_name = "NAME")]
    workspace: Option<String>,
}

#[derive(Args)]
struct GenerateArgs {
    #[command(flatten)]
    workspace: WorkspaceArgs,

    /// Approve the migration plan without asking. Plans with destructive
    /// changes need --allow-destructive
    #[arg(short = 'y', long)]
    yes: bool,

    /// Approve the migration plan, including changes that drop or rewrite
    /// data. Implies --yes
    #[arg(long)]
    allow_destructive: bool,

    /// Write the migration files without applying them. Apply them later
    /// with `dinoco migrate run`
    #[arg(long)]
    create_only: bool,

    /// Accept every rename the planner inferred
    #[arg(long)]
    accept_renames: bool,

    /// Accept one inferred rename: TABLE.OLD=NEW for a column, OLD=NEW for a
    /// table. Repeatable
    #[arg(long = "rename", value_name = "MAPPING")]
    renames: Vec<String>,

    /// Treat an inferred rename as a removal plus an addition instead (same
    /// format as --rename). Repeatable
    #[arg(long = "no-rename", value_name = "MAPPING")]
    rejected_renames: Vec<String>,

    /// Rewrite rows that still use a removed enum value: ENUM.VALUE=REPLACEMENT.
    /// Repeatable
    #[arg(long = "replace-enum-value", value_name = "MAPPING")]
    enum_replacements: Vec<String>,

    /// Approve repairing schema drift: changes made to the database outside
    /// Dinoco migrations
    #[arg(long)]
    repair_drift: bool,

    /// Approve adopting existing tables that already match the schema as the
    /// migration baseline
    #[arg(long)]
    adopt_baseline: bool,
}

/// Runs the command line and returns the process exit code, after printing
/// any error.
pub async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => ExitCode::from(error::report(&error)),
    }
}

pub async fn run() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    let cli = Cli::parse();
    ui::configure(cli.no_color);
    prompt::configure(cli.non_interactive);

    match cli.command {
        Commands::Init(args) => commands::init::run(InitOptions {
            database: args.database.map(|database| match database {
                DatabaseArg::Postgresql => "postgresql",
                DatabaseArg::Mysql => "mysql",
                DatabaseArg::Sqlite => "sqlite",
            }),
            connection: args.connection.map(|connection| match connection {
                ConnectionArg::Direct => "direct",
                ConnectionArg::Pgbouncer => "pgbouncer",
            }),
        })?,
        Commands::Migrate(MigrateCommands::Generate(args)) => {
            commands::migrate::generate_with(GenerateOptions {
                workspace: args.workspace.workspace,
                yes: args.yes,
                allow_destructive: args.allow_destructive,
                create_only: args.create_only,
                accept_renames: args.accept_renames,
                renames: args.renames,
                rejected_renames: args.rejected_renames,
                enum_replacements: args.enum_replacements,
                repair_drift: args.repair_drift,
                adopt_baseline: args.adopt_baseline,
            })
            .await?
        }
        Commands::Migrate(MigrateCommands::Run(args)) => commands::migrate::run(args.workspace).await?,
        Commands::Models(ModelsCommands::Generate(args)) => commands::models::generate(args.workspace).await?,
    }

    Ok(())
}
