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

pub use pest::Span;

#[derive(Parser)]
#[grammar = "schema.pest"]
pub struct DinocoParser;

pub fn render_error(error: &DinocoError, source: &str) -> String {
    fn get_offset(source: &str, line: usize, col: usize) -> usize {
        source.lines().take(line - 1).map(|l| l.len() + 1).sum::<usize>() + (col - 1)
    }

    let mut out = Vec::new();

    let offset = get_offset(source, error.start_line, error.start_column);
    let end_offset = get_offset(source, error.end_line, error.end_column);

    Report::build(ReportKind::Error, "schema.dinoco", offset)
        .with_code("E001")
        .with_message(&error.message)
        .with_label(Label::new(("schema.dinoco", offset..end_offset)).with_message("happened here").with_color(Color::Red))
        .finish()
        .write(("schema.dinoco", Source::from(source)), &mut out)
        .unwrap();

    String::from_utf8_lossy(&out).to_string()
}

pub fn compile<'a>(raw_input: &'a str) -> Result<(Schema<'a>, ParsedSchema), Vec<DinocoError>> {
    let schema = parser::parse_schema(raw_input)?;
    let parsed = validator::validate_schema(&schema)?;

    Ok((schema, parsed))
}

pub fn compile_only_ast<'a>(raw_input: &'a str) -> Result<Schema<'a>, Vec<DinocoError>> {
    let schema = parser::parse_schema(raw_input)?;

    Ok(schema)
}
