#![allow(non_snake_case)]

extern crate etanol_databases;
extern crate etanol_utils;

mod commands;

mod generation;
mod migrations;
mod utils;
use utils::*;

use commands::migrate;

use ansi_term::Colour;

fn main() {
    let mut args: Vec<String> = std::env::args().collect();
    args.remove(0);

    if args.len() > 1 {
        match args[0].as_str() {
            "migrate" => migrate::parse(&args),
            _ => help(),
        }

        return;
    }

    help()
}

fn help() {
    let white = Colour::White.bold();
    let gray = Colour::RGB(142, 142, 142);

    println!("{}", white.paint("Usage\n"));
    println!(
        "  {}{}\n",
        gray.paint("$ "),
        "etanol [command] [subcommand]"
    );

    println!("{}", white.paint("Examples\n"));
    migrate::usage_migrate(false);

    std::process::exit(1);
}
