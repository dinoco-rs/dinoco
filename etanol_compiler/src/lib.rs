#![allow(unreachable_patterns)]
#![allow(unused_assignments)]

mod ast;
mod parsed_ast;
mod parser;
mod validator;

use ariadne::{Color, Label, Report, ReportKind, Source};
use pest_derive::Parser;

pub use ast::*;
pub use parsed_ast::*;

#[derive(Parser)]
#[grammar = "schema.pest"]
pub struct EtanolParser;

pub fn render_error(error: &EtanolError, source: &str) -> String {
    fn get_offset(source: &str, line: usize, col: usize) -> usize {
        source.lines().take(line - 1).map(|l| l.len() + 1).sum::<usize>() + (col - 1)
    }

    let mut out = Vec::new();

    let offset = get_offset(source, error.start_line, error.start_column);
    let end_offset = get_offset(source, error.end_line, error.end_column);

    Report::build(ReportKind::Error, "schema.etanol", offset)
        .with_code("E001")
        .with_message(&error.message)
        .with_label(Label::new(("schema.etanol", offset..end_offset)).with_message("happened here").with_color(Color::Red))
        .finish()
        .write(("schema.etanol", Source::from(source)), &mut out)
        .unwrap();

    String::from_utf8_lossy(&out).to_string()
}

pub fn compile<'a>(raw_input: &'a str) -> Result<ParsedSchema, Vec<EtanolError>> {
    let schema = parser::parse_schema(raw_input)?;
    let parsed = validator::validate_schema(&schema)?;

    Ok(parsed)
}
