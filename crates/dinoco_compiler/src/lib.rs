mod ast;
mod error;
mod parser;

pub use ast::*;
pub use error::*;

pub type CompileResult<T> = Result<T, CompileError>;

/// Parses a schema without running semantic validation.
///
/// Editors can use this to report every semantic issue in a syntactically valid
/// document instead of stopping at the compiler's first validation error.
pub fn parse(source: &str) -> CompileResult<Schema> {
    parser::parse_schema(source)
}

pub fn compile(source: &str) -> CompileResult<Schema> {
    let schema = parse(source)?;
    parser::validate_schema(&schema)?;
    Ok(schema)
}
