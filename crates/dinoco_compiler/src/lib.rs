mod ast;
mod error;
mod parser;
mod resolver;

use std::path::Path;

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
    if let Some(import) = schema.imports().next() {
        return Err(CompileError::at(
            "Imports require file-based compilation starting from `schema.dinoco`",
            &import.origin,
        ));
    }
    if let Some(import) = schema.config_imports().next() {
        return Err(CompileError::at(
            "`config.imports` requires file-based compilation starting from `schema.dinoco`",
            &import.origin,
        ));
    }
    parser::validate_schema(&schema)?;
    Ok(schema)
}

/// Compiles `schema.dinoco` and its complete import tree.
pub fn compile_file(path: impl AsRef<Path>) -> CompileResult<Schema> {
    resolver::compile_file(path.as_ref())
}
