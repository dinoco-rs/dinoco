use std::fmt;

use crate::SourceOrigin;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelatedDiagnostic {
    pub message: String,
    pub file: String,
    pub line: usize,
    pub column: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompileError {
    pub message: String,
    pub file: Option<String>,
    pub line: usize,
    pub column: usize,
    pub related: Vec<RelatedDiagnostic>,
}

impl CompileError {
    pub(crate) fn new(message: impl Into<String>, line: usize, column: usize) -> Self {
        Self { message: message.into(), file: None, line, column, related: Vec::new() }
    }

    pub(crate) fn at(message: impl Into<String>, origin: &SourceOrigin) -> Self {
        Self {
            message: message.into(),
            file: Some(origin.file.clone()),
            line: origin.line,
            column: origin.column,
            related: Vec::new(),
        }
    }

    pub(crate) fn with_file(mut self, file: impl Into<String>) -> Self {
        self.file = Some(file.into());
        self
    }

    pub(crate) fn with_related(mut self, message: impl Into<String>, origin: &SourceOrigin) -> Self {
        self.related.push(RelatedDiagnostic {
            message: message.into(),
            file: origin.file.clone(),
            line: origin.line,
            column: origin.column,
        });
        self
    }
}

impl fmt::Display for CompileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)?;
        if let Some(file) = &self.file {
            write!(f, "\n --> {file}:{}:{}", self.line, self.column)?;
        } else {
            write!(f, " at {}:{}", self.line, self.column)?;
        }
        for related in &self.related {
            write!(f, "\n  = {}\n --> {}:{}:{}", related.message, related.file, related.line, related.column)?;
        }
        Ok(())
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
