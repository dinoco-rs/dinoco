use std::env;
use std::io::{self, Read};

fn main() {
    let result = match env::args().nth(1) {
        Some(path) => dinoco_compiler::compile_file(path),
        None => {
            let mut source = String::new();
            if let Err(err) = io::stdin().read_to_string(&mut source) {
                eprintln!("failed to read stdin: {err}");
                std::process::exit(1);
            }
            dinoco_compiler::compile(&source)
        }
    };

    match result {
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
