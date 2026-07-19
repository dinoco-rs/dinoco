use std::env;
use std::fs;
use std::io::{self, Read};

fn main() {
    let source = match env::args().nth(1) {
        Some(path) => match fs::read_to_string(&path) {
            Ok(source) => source,
            Err(err) => {
                eprintln!("failed to read `{path}`: {err}");
                std::process::exit(1);
            }
        },
        None => {
            let mut source = String::new();
            if let Err(err) = io::stdin().read_to_string(&mut source) {
                eprintln!("failed to read stdin: {err}");
                std::process::exit(1);
            }
            source
        }
    };

    match dinoco_compiler::compile(&source) {
        Ok(schema) => {
            println!(
                "compiled: {} config, {} enums, {} models",
                usize::from(schema.config().is_some()),
                schema.enums().count(),
                schema.models().count()
            );
        }
        Err(err) => {
            eprintln!("{err}");
            std::process::exit(1);
        }
    }
}
