//! Actionable CLI errors.
//!
//! A [`CliError`] says what failed, the target it failed on, why (when that is
//! known) and what to do next, and maps to an exit code. Errors that are not a
//! `CliError` are printed with their cause chain and exit with code 1.

use std::fmt;

use crate::ui;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    /// The operation failed. Exit code 1.
    Failure,
    /// The command line asks for something that cannot be done as written.
    /// Exit code 2.
    InvalidUsage,
    /// A decision has to be made, but no terminal is available to ask for it
    /// and no option supplied it. Exit code 2.
    DecisionRequired,
    /// The user cancelled. Exit code 130, as for an interrupted command.
    Cancelled,
}

#[derive(Debug, Clone)]
pub struct CliError {
    kind: ErrorKind,
    title: String,
    details: Vec<(String, String)>,
    reason: Option<String>,
    hints: Vec<String>,
}

impl CliError {
    pub fn new(kind: ErrorKind, title: impl Into<String>) -> Self {
        Self { kind, title: title.into(), details: Vec::new(), reason: None, hints: Vec::new() }
    }

    pub fn failure(title: impl Into<String>) -> Self {
        Self::new(ErrorKind::Failure, title)
    }

    pub fn usage(title: impl Into<String>) -> Self {
        Self::new(ErrorKind::InvalidUsage, title)
    }

    pub fn decision_required(title: impl Into<String>) -> Self {
        Self::new(ErrorKind::DecisionRequired, title)
    }

    pub fn cancelled(title: impl Into<String>) -> Self {
        Self::new(ErrorKind::Cancelled, title)
    }

    /// A labelled fact about the failed operation, such as its workspace.
    pub fn detail(mut self, label: impl Into<String>, value: impl Into<String>) -> Self {
        self.details.push((label.into(), value.into()));
        self
    }

    /// Why the operation could not continue.
    pub fn reason(mut self, reason: impl Into<String>) -> Self {
        self.reason = Some(reason.into());
        self
    }

    /// Something the user can do next.
    pub fn hint(mut self, hint: impl Into<String>) -> Self {
        self.hints.push(hint.into());
        self
    }

    pub fn kind(&self) -> ErrorKind {
        self.kind
    }

    pub fn exit_code(&self) -> u8 {
        match self.kind {
            ErrorKind::Failure => 1,
            ErrorKind::InvalidUsage | ErrorKind::DecisionRequired => 2,
            ErrorKind::Cancelled => 130,
        }
    }

    /// Prints the error, then `causes` (the underlying failures, outermost
    /// first), then the hints.
    pub fn render(&self, causes: &[String]) {
        ui::begin_error_report();
        if self.kind == ErrorKind::Cancelled {
            ui::warning(&self.title);
        } else {
            ui::error(&self.title);
        }
        if !self.details.is_empty() {
            ui::error_text("");
            ui::error_fields(&self.details);
        }
        if let Some(reason) = &self.reason {
            ui::error_text("");
            ui::error_text(reason);
        }
        if !causes.is_empty() {
            ui::error_text("");
            ui::error_text("Caused by:");
            for cause in causes {
                ui::error_text(format!("  {cause}"));
            }
        }
        if !self.hints.is_empty() {
            ui::error_text("");
            for hint in &self.hints {
                ui::help(hint);
            }
        }
    }
}

impl fmt::Display for CliError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.title)?;
        if let Some(reason) = &self.reason {
            write!(formatter, " {reason}")?;
        }
        Ok(())
    }
}

impl std::error::Error for CliError {}

/// Prints `error` and returns the process exit code for it.
pub fn report(error: &anyhow::Error) -> u8 {
    // A `CliError` is either the error itself or the context of the failure
    // it explains; every other message in the chain is printed as its cause.
    if let Some(cli_error) = error.downcast_ref::<CliError>() {
        let own = cli_error.to_string();
        let causes = error.chain().map(ToString::to_string).filter(|message| *message != own).collect::<Vec<_>>();
        cli_error.render(&causes);
        return cli_error.exit_code();
    }

    ui::begin_error_report();
    ui::error(error.to_string());
    let causes = error.chain().skip(1).map(ToString::to_string).collect::<Vec<_>>();
    if !causes.is_empty() {
        ui::error_text("");
        ui::error_text("Caused by:");
        for cause in causes {
            ui::error_text(format!("  {cause}"));
        }
    }
    1
}

/// Attaches a [`CliError`] to a lower-level failure, keeping the original
/// error as its printed cause.
pub trait WithCliError<T> {
    fn cli_error(self, error: impl FnOnce() -> CliError) -> anyhow::Result<T>;
}

impl<T, E> WithCliError<T> for Result<T, E>
where
    E: Into<anyhow::Error>,
{
    fn cli_error(self, error: impl FnOnce() -> CliError) -> anyhow::Result<T> {
        self.map_err(|source| source.into().context(error()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exit_codes_separate_failures_from_missing_decisions_and_cancellation() {
        assert_eq!(CliError::failure("failed").exit_code(), 1);
        assert_eq!(CliError::usage("bad option").exit_code(), 2);
        assert_eq!(CliError::decision_required("needs an answer").exit_code(), 2);
        assert_eq!(CliError::cancelled("cancelled").exit_code(), 130);
    }

    #[test]
    fn a_cli_error_attached_as_context_decides_the_exit_code() {
        let error = Err::<(), _>(std::io::Error::other("disk full"))
            .cli_error(|| CliError::decision_required("A workspace must be selected."))
            .unwrap_err();
        assert_eq!(report(&error), 2);
        assert_eq!(report(&anyhow::anyhow!("plain failure")), 1);
    }
}
