mod ast;
mod error;
mod parser;

pub use ast::*;
pub use error::*;

pub type CompileResult<T> = Result<T, CompileError>;

pub fn compile(source: &str) -> CompileResult<Schema> {
    parser::parse_schema(source)
}
