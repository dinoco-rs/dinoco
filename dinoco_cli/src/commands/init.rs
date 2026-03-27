use dinoco_codegen::dinoco::{DinocoConfig, DinocoDatabase, DinocoDatabaseUrl, DinocoSchema};
use dinoco_formatter::format_from_raw;
use dinoco_macros::ternary;

use colored::Colorize;
use inquire::validator::Validation;
use inquire::{Confirm, Select, Text};

pub fn init_command() {
    let exists = std::fs::exists("dinoco/schema.dinoco").unwrap_or(false);
    if exists {
        let rewrite = Confirm::new("Dinoco environment is already initialized. Do you want to overwrite it?")
            .with_default(false)
            .prompt();

        match rewrite {
            Ok(true) => {}
            Ok(false) => return println!("{}", "Initialization cancelled.".yellow()),
            Err(_) => return,
        }
    }

    let database_prompt = Select::new("Which database will you use?", vec!["PostgreSQL", "MySQL", "SQLite"]).prompt();
    let database = match database_prompt {
        Ok(db) => db,
        Err(_) => return println!("{}", "You must select a database!".red()),
    };

    let connection_type_prompt = Select::new("How do you want to provide the connection string?", vec!["Environment variable", "Static string"]).prompt();
    let connection_type = match connection_type_prompt {
        Ok(ct) => ct,
        Err(_) => return println!("{}", "You must select a connection type!".red()),
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
                return Ok(Validation::Invalid("Environment variables can only contain letters, numbers, and underscores.".into()));
            }
        }

        Ok(Validation::Valid)
    };

    let prompt_message = ternary!(is_env, "What is the environment variable name?", "What is the connection string?");
    let connection_url_prompt = Text::new(prompt_message).with_validator(input_validator).prompt();

    let connection_url = match connection_url_prompt {
        Ok(url) => url,
        Err(_) => return println!("{}", "You must provide a valid input!".red()),
    };

    let with_replicas_prompt = Confirm::new("Do you want to use read replicas?").with_default(false).prompt();
    let with_replicas = match with_replicas_prompt {
        Ok(val) => val,
        Err(_) => return println!("{}", "We need to know if you want to use read replicas.".red()),
    };

    let mut replicas_amount = 0;

    if with_replicas {
        let replica_validator = |input: &str| match input.trim().parse::<i32>() {
            Ok(val) if val > 0 => Ok(Validation::Valid),
            _ => Ok(Validation::Invalid("Please enter a valid number greater than 0.".into())),
        };

        let amount_prompt = Text::new("How many replicas?").with_validator(replica_validator).prompt();
        match amount_prompt {
            Ok(amount) => replicas_amount = amount.trim().parse::<i32>().unwrap(),
            Err(_) => return println!("{}", "We need to know how many replicas you want to use!".red()),
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
        ternary!(is_env, DinocoDatabaseUrl::Env(connection_url.clone()), DinocoDatabaseUrl::String(connection_url.clone())),
        replicas,
    );
    let schema = DinocoSchema::new(config);

    if exists {
        std::fs::remove_dir_all("dinoco").unwrap();
    }

    std::fs::create_dir_all("dinoco").unwrap();

    let formatted_schema = format_from_raw(&schema.to_string()).unwrap_or(schema.to_string());
    std::fs::write("dinoco/schema.dinoco", formatted_schema).unwrap();

    println!("\n{} {}", "✔".green().bold(), "Your Dinoco environment was successfully created!".green());
    println!(
        "{} {}",
        "📚 Next steps: Check out the documentation at".bright_black(),
        "https://dinoco.io/docs".cyan().underline()
    );
}
