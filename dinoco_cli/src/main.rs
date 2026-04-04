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

    #[command(about = "Rul all migrations")]
    Run {},
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
                if let Err(data) = generate_migrate().await {
                    eprintln!("❌ Failed to generate migration.");
                    eprintln!("👉 Details: {}", data);

                    eprintln!("\n💡 Possible causes:");
                    eprintln!("- Database connection failure");
                    eprintln!("- Invalid or inconsistent schema");
                    eprintln!("- Insufficient database permissions");

                    eprintln!("\n🛠️ Suggestions:");
                    eprintln!("- Check the DATABASE_URL environment variable");
                    eprintln!("- Ensure the database is accessible");
                    eprintln!("- Review recent schema changes");
                }
            }

            &MigrateCommands::Rollback {} => {
                if let Err(err) = rollback_migration().await {
                    eprintln!("❌ Failed to execute rollback.");
                    eprintln!("👉 Details: {}", err);

                    eprintln!("\n💡 Possible causes:");
                    eprintln!("- No migrations available to rollback");
                    eprintln!("- Inconsistent migration state");
                    eprintln!("- Database connection failure");

                    eprintln!("\n🛠️ Suggestions:");
                    eprintln!("- Check the migration history");
                    eprintln!("- Verify the database connection");
                    eprintln!("- Run a migration status command first");
                }
            }

            &MigrateCommands::Run {} => {
                if let Err(err) = run_migrations().await {
                    eprintln!("❌ Failed to execute rollback.");
                    eprintln!("👉 Details: {}", err);

                    eprintln!("\n💡 Possible causes:");
                    eprintln!("- No migrations available to rollback");
                    eprintln!("- Inconsistent migration state");
                    eprintln!("- Database connection failure");

                    eprintln!("\n🛠️ Suggestions:");
                    eprintln!("- Check the migration history");
                    eprintln!("- Verify the database connection");
                    eprintln!("- Run a migration status command first");
                }
            }
        },
    }
}
