use clap::{Parser, Subcommand};
use colored::Colorize;

mod commands;
use commands::*;

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
    }
}
