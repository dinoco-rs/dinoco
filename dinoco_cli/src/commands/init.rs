use dinoco_codegen::dinoco::{DinocoConfig, DinocoDatabase, DinocoDatabaseUrl, DinocoSchema};
use dinoco_formatter::format_from_raw;

use colored::Colorize;
use inquire::validator::Validation;
use inquire::{Confirm, Select, Text};
use std::path::Path;

use crate::ternary;
use crate::utils::{env_prompt_bool, env_prompt_string};

pub fn init_command() {
    let exists = Path::new("dinoco/schema.dinoco").exists();

    if exists {
        println!("\n{} {}", "⚠".yellow().bold(), "Dinoco project already exists in this directory.".yellow());
        let rewrite = match env_prompt_bool("DINOCO_CLI_INIT_OVERWRITE") {
            Some(value) => Ok(value),
            None => Confirm::new("Do you want to overwrite the existing configuration?").with_default(false).prompt(),
        };

        match rewrite {
            Ok(true) => {}
            Ok(false) => {
                println!("{} {}", "✗".red().bold(), "Initialization cancelled.".white());
                return;
            }
            Err(_) => return,
        }
    }

    let database_prompt = match env_prompt_string("DINOCO_CLI_INIT_DATABASE") {
        Some(database) => Ok(database),
        None => Select::new("Which database will you use?", vec!["PostgreSQL", "MySQL"]).prompt().map(str::to_string),
    };
    let database = match database_prompt {
        Ok(db) => db,
        Err(_) => {
            println!("\n{} {}", "✖".red().bold(), "Database selection cancelled.".white());
            return;
        }
    };

    let connection_type_prompt = match env_prompt_string("DINOCO_CLI_INIT_CONNECTION_TYPE") {
        Some(connection_type) => Ok(connection_type),
        None => Select::new(
            "How do you want to provide the connection string?",
            vec!["Environment variable", "Static string"],
        )
        .prompt()
        .map(str::to_string),
    };
    let connection_type = match connection_type_prompt {
        Ok(ct) => ct,
        Err(_) => {
            println!("\n{} {}", "✖".red().bold(), "Connection type selection cancelled.".white());
            return;
        }
    };

    let is_env = connection_type == "Environment variable";
    let input_validator = move |input: &str| {
        if input.trim().is_empty() {
            return Ok(Validation::Invalid("The value cannot be empty.".into()));
        }

        if is_env {
            if input.contains(' ') {
                return Ok(Validation::Invalid("Environment variable names cannot contain spaces.".into()));
            }

            if !input.chars().all(|c| c.is_alphanumeric() || c == '_') {
                return Ok(Validation::Invalid(
                    "Environment variables can only contain letters, numbers, and underscores.".into(),
                ));
            }
        }

        Ok(Validation::Valid)
    };

    let prompt_message = ternary!(is_env, "What is the environment variable name?", "What is the connection string?");
    let connection_url_prompt = match env_prompt_string("DINOCO_CLI_INIT_CONNECTION_URL") {
        Some(connection_url) => Ok(connection_url),
        None => Text::new(prompt_message).with_validator(input_validator).prompt(),
    };
    let connection_url = match connection_url_prompt {
        Ok(url) => url,
        Err(_) => {
            println!("\n{} {}", "✖".red().bold(), "Input cancelled.".white());
            return;
        }
    };

    let replicas_prompt = match env_prompt_bool("DINOCO_CLI_INIT_WITH_REPLICAS") {
        Some(with_replicas) => Ok(with_replicas),
        None => Confirm::new("Do you want to use read replicas?").with_default(false).prompt(),
    };
    let with_replicas = match replicas_prompt {
        Ok(val) => val,
        Err(_) => {
            println!("\n{} {}", "✖".red().bold(), "Replica configuration cancelled.".white());
            return;
        }
    };

    let mut replicas_amount = 0;

    if with_replicas {
        let replica_validator = |input: &str| match input.trim().parse::<u32>() {
            Ok(val) if val > 0 => Ok(Validation::Valid),
            _ => Ok(Validation::Invalid("Please enter a valid number greater than 0.".into())),
        };

        let replicas_amount_prompt = match env_prompt_string("DINOCO_CLI_INIT_REPLICAS_AMOUNT") {
            Some(amount) => Ok(amount),
            None => Text::new("How many replicas?").with_validator(replica_validator).prompt(),
        };

        match replicas_amount_prompt {
            Ok(amount) => replicas_amount = amount.trim().parse::<u32>().unwrap_or(0),
            Err(_) => {
                println!("\n{} {}", "✖".red().bold(), "Replica amount cancelled.".white());
                return;
            }
        };
    }

    let mut replicas = vec![];

    if replicas_amount > 0 {
        for i in 0..replicas_amount {
            replicas.push(ternary!(
                is_env,
                DinocoDatabaseUrl::Env(format!("{}_REPLICA_{}", connection_url.clone(), i + 1)),
                DinocoDatabaseUrl::String("".to_string())
            ))
        }
    }

    let config = DinocoConfig::new(
        DinocoDatabase::from_str(&database.to_lowercase()).unwrap(),
        ternary!(
            is_env,
            DinocoDatabaseUrl::Env(connection_url.clone()),
            DinocoDatabaseUrl::String(connection_url.clone())
        ),
        replicas,
    );
    let schema = DinocoSchema::new(config);

    if exists {
        if let Err(e) = std::fs::remove_dir_all("dinoco") {
            println!(
                "\n{} {}\n  {} {}",
                "✖".red().bold(),
                "Failed to remove existing directory.".bold(),
                "Reason:".yellow().bold(),
                e
            );
            return;
        }
    }

    if let Err(e) = std::fs::create_dir_all("dinoco") {
        println!(
            "\n{} {}\n  {} {}",
            "✖".red().bold(),
            "Failed to create dinoco directory.".bold(),
            "Reason:".yellow().bold(),
            e
        );
        return;
    }

    let formatted_schema = format_from_raw(&schema.to_string()).unwrap_or(schema.to_string());

    if let Err(e) = std::fs::write("dinoco/schema.dinoco", formatted_schema) {
        println!(
            "\n{} {}\n  {} {}",
            "✖".red().bold(),
            "Failed to write schema file.".bold(),
            "Reason:".yellow().bold(),
            e
        );
        return;
    }

    println!("\n{} {}", "✔".green().bold(), "Your Dinoco environment was successfully created!".white());
    println!("  {} Schema created at: {}", "→".cyan().bold(), "dinoco/schema.dinoco".blue());
    println!(
        "\n{} {}",
        "📚 Next steps: Check out the documentation at".bright_black(),
        "https://dinoco.io/docs".cyan().underline()
    );
}
