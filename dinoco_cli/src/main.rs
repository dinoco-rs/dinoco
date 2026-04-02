use clap::{Parser, Subcommand};
use colored::Colorize;

mod commands;
mod utils;

mod helpers;

use commands::*;
use helpers::*;

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
    Migrate(MigrateCommands),
}

#[derive(Subcommand)]
enum MigrateCommands {
    #[command(about = "Generate a migration from schema")]
    Generate {},

    #[command(about = "Rollback last migration")]
    Rollback {},
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

        Commands::Migrate(command) => match command {
            &MigrateCommands::Generate {} => {
                let _result = generate_migrate().await;
            }
            &MigrateCommands::Rollback {} => {
                let _result = rollback_migration().await;
            }
        },
    }
}
