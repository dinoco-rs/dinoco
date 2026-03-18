use pest::Parser;
use pest_derive::Parser;

mod parser;
mod structs;

use structs::*;

use crate::parser::parse_schema;

#[derive(Parser)]
#[grammar = "../schema.pest"]
struct EtanolParser;

pub fn compile() {
    let result = parse_schema(&std::fs::read_to_string(&"schema.etanol").unwrap());

    println!("{:#?}", result);
}
