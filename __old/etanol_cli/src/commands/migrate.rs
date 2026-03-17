use crate::migrations::{migrate_deploy, migrate_dev};

use ansi_term::Colour;

pub fn parse(args: &Vec<String>) {
    if args.len() > 2 {
        match args[1].as_str() {
            "dev" => {
                if args.len() > 2 {
                    let name = args[2].clone();

                    if name == "--name" {
                        if args.len() > 3 {
                            let mut name = String::new();

                            for letter in args[3].trim().split("") {
                                if letter.to_uppercase() == letter {
                                    if name.is_empty() {
                                        name.push_str(&letter.to_lowercase());

                                        continue;
                                    }

                                    name.push_str("_");
                                }

                                name.push_str(&letter.to_lowercase());
                            }

                            name.pop();

                            migrate_dev(name);

                            return;
                        }
                    }
                }
            }
            "deploy" => migrate_deploy(),
            _ => {}
        }
    }

    usage_migrate(true)
}

pub fn usage_migrate(usage: bool) {
    let white = Colour::White.bold();
    let gray = Colour::RGB(142, 142, 142);

    if usage {
        println!("{}", white.paint("Usage\n"));
    }

    let examples = vec![
        (
            "Create migrations from your schema",
            "etanol migrate dev --name [name]",
        ),
        ("Run migrations from your schema", "etanol migrate deploy"),
    ];

    for (help, command) in examples {
        println!("  {}", help);
        println!("  {}{}\n", gray.paint("$ "), command);
    }

    std::process::exit(1);
}
