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

    match dinoco_formatter::format_from_raw(&source) {
        Ok(formatted) => print!("{formatted}"),
        Err(err) => {
            eprintln!("{err}");
            std::process::exit(1);
        }
    }
}
