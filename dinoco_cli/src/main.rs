use clap::{Parser, Subcommand};
use colored::Colorize;

mod commands;
mod utils;

mod helpers;

use commands::*;
use helpers::*;

#[derive(Parser)]
#[command(name = "dinoco")]
#[command(
    about = "Dinoco is a modern type-safe database engine for querying, modeling and managing data (https://dinoco.io)"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    #[command(about = "Initialize the Dinoco environment to configure your database")]
    Init {},

    #[command(subcommand)]
    Database(DatabaseCommands),

    #[command(subcommand)]
    Migrate(MigrateCommands),

    #[command(subcommand)]
    Models(ModelsCommands),

    #[command(subcommand)]
    Schema(SchemaCommands),
}

#[derive(Subcommand)]
enum DatabaseCommands {
    #[command(about = "Reset the configured database")]
    Reset {},
}

#[derive(Subcommand)]
enum MigrateCommands {
    #[command(about = "Generate a migration from the current schema")]
    Generate {
        #[arg(long, help = "Apply the generated migration immediately and generate Rust models")]
        apply: bool,
    },

    #[command(about = "Rollback the last migration")]
    Rollback {},

    #[command(about = "Run all pending migrations")]
    Run {},
}

#[derive(Subcommand)]
enum ModelsCommands {
    #[command(about = "Generate Rust models from the latest migration stored in the database")]
    Generate {},
}

#[derive(Subcommand)]
enum SchemaCommands {
    #[command(about = "Restore schema.dinoco from the latest local migration")]
    Restore {
        #[arg(help = "Optional migration name to restore from")]
        name: Option<String>,
    },
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
            &DatabaseCommands::Reset {} => {
                if let Err(err) = reset_database().await {
                    eprintln!("❌ Failed to reset database.");
                    eprintln!("👉 Details: {}", err);

                    eprintln!("\n💡 Possible causes:");
                    eprintln!("- Database connection failure");
                    eprintln!("- Insufficient database permissions");
                    eprintln!("- An unexpected error while dropping tables");

                    eprintln!("\n🛠️ Suggestions:");
                    eprintln!("- Check the DATABASE_URL environment variable");
                    eprintln!("- Verify the database connection");
                    eprintln!("- Confirm the current user can drop database objects");
                }
            }
        },

        Commands::Migrate(command) => match command {
            MigrateCommands::Generate { apply } => {
                if let Err(data) = generate_migrate(*apply).await {
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
                    eprintln!("❌ Failed to roll back the migration.");
                    eprintln!("👉 Details: {}", err);

                    eprintln!("\n💡 Possible causes:");
                    eprintln!("- No migrations are available to roll back");
                    eprintln!("- The migration history is inconsistent");
                    eprintln!("- Database connection failure");

                    eprintln!("\n🛠️ Suggestions:");
                    eprintln!("- Check the migration history");
                    eprintln!("- Verify the database connection");
                    eprintln!("- Re-run the latest migration if needed");
                }
            }

            &MigrateCommands::Run {} => {
                if let Err(err) = run_migrations().await {
                    eprintln!("❌ Failed to run pending migrations.");
                    eprintln!("👉 Details: {}", err);

                    eprintln!("\n💡 Possible causes:");
                    eprintln!("- The migration files are invalid");
                    eprintln!("- The migration history is inconsistent");
                    eprintln!("- Database connection failure");

                    eprintln!("\n🛠️ Suggestions:");
                    eprintln!("- Check the migration history");
                    eprintln!("- Verify the database connection");
                    eprintln!("- Review the latest generated migration");
                }
            }
        },

        Commands::Models(command) => match command {
            ModelsCommands::Generate {} => {
                if let Err(err) = generate_models_from_latest_migration().await {
                    eprintln!("❌ Failed to generate models.");
                    eprintln!("👉 Details: {}", err);

                    eprintln!("\n💡 Possible causes:");
                    eprintln!("- No migrations were applied to the database");
                    eprintln!("- The latest local migration files are missing");
                    eprintln!("- Database connection failure");

                    eprintln!("\n🛠️ Suggestions:");
                    eprintln!("- Run `dinoco migrate generate` first");
                    eprintln!("- Run `dinoco migrate run` if migrations are pending");
                    eprintln!("- Verify the database connection");
                }
            }
        },

        Commands::Schema(command) => match command {
            SchemaCommands::Restore { name } => {
                if let Err(err) = restore_schema_from_migration(name.as_deref()) {
                    eprintln!("❌ Failed to restore schema.dinoco.");
                    eprintln!("👉 Details: {}", err);

                    eprintln!("\n💡 Possible causes:");
                    eprintln!("- No local migrations are available");
                    eprintln!("- The latest migration is missing schema.bin");
                    eprintln!("- The migration metadata is corrupted");

                    eprintln!("\n🛠️ Suggestions:");
                    eprintln!("- Run `dinoco migrate generate` first");
                    eprintln!("- Check whether the latest migration contains schema.bin");
                    eprintln!("- Re-generate the latest migration if needed");
                }
            }
        },
    }
}
