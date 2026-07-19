use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompileError {
    pub message: String,
    pub line: usize,
    pub column: usize,
}

impl CompileError {
    pub(crate) fn new(message: impl Into<String>, line: usize, column: usize) -> Self {
        Self { message: message.into(), line, column }
    }
}

impl fmt::Display for CompileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} at {}:{}", self.message, self.line, self.column)
    }
}

impl std::error::Error for CompileError {}

impl<R: pest::RuleType> From<pest::error::Error<R>> for CompileError {
    fn from(error: pest::error::Error<R>) -> Self {
        let (line, column) = match error.line_col {
            pest::error::LineColLocation::Pos((line, column)) => (line, column),
            pest::error::LineColLocation::Span((line, column), _) => (line, column),
        };

        Self::new(error.to_string(), line, column)
    }
}
