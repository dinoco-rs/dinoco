use clap::{Parser, Subcommand};
use colored::Colorize;

mod commands;
mod utils;

use commands::*;
use utils::*;

#[derive(Parser)]
#[command(name = "dinoco")]
#[command(about = "Dinoco is a modern type-safe database engine for querying, modeling and managing data (https://dinoco.io)")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    #[command(about = "Initialize the Dinoco environment to configure your database")]
    Init {},

    #[command(subcommand)]
    Database(DbCommands),

    #[command(subcommand)]
    Migrate(MigrateCommands),
}

#[derive(Subcommand)]
enum MigrateCommands {
    #[command(about = "Generate a migration from schema")]
    Create {},
}

#[derive(Subcommand)]
enum DbCommands {
    #[command(about = "Import a schema from current database")]
    Import {},
}

#[tokio::main]
async fn main() {
    let env = dotenvy::dotenv().ok();
    if env.is_some() {
        println!("{} {}", "ℹ".blue(), "Successfully loaded .env file!".bright_black());
    }

    let cli = Cli::parse();

    match &cli.command {
        Commands::Init {} => init_command(),
        Commands::Database(command) => match command {
            DbCommands::Import {} => database_import_command().await,
        },

        Commands::Migrate(command) => match command {
            &MigrateCommands::Create {} => migrate_create().await,
        },
    }
}
