//! Interactive decisions.
//!
//! Prompts are only shown when both standard input and standard error are
//! terminals, `--non-interactive` was not passed and `CI` is not set. Without
//! a terminal, commands never wait for input: they take the decision from a
//! command-line option, or stop with a [`CliError`] naming the option to pass.

use std::fmt;
use std::io::IsTerminal;
use std::sync::atomic::{AtomicBool, Ordering};

use inquire::error::InquireError;
use inquire::ui::{Color, RenderConfig, StyleSheet, Styled};
use inquire::{Confirm, Select};

use crate::error::CliError;

static INTERACTIVE: AtomicBool = AtomicBool::new(false);

const SELECT_HELP: &str = "Use ↑/↓ to navigate, Enter to confirm, Esc to cancel.";
const CONFIRM_HELP: &str = "Type y or n, then press Enter. Esc cancels.";

/// Decides, once at startup, whether prompts may be shown.
pub fn configure(non_interactive: bool) {
    let interactive =
        !non_interactive && !running_in_ci() && std::io::stdin().is_terminal() && std::io::stderr().is_terminal();
    INTERACTIVE.store(interactive, Ordering::Relaxed);

    let config = if crate::ui::prompt_colors() {
        RenderConfig::default_colored()
            .with_prompt_prefix(Styled::new("?").with_fg(Color::LightCyan))
            .with_answered_prompt_prefix(Styled::new("✓").with_fg(Color::LightGreen))
            .with_highlighted_option_prefix(Styled::new(">").with_fg(Color::LightCyan))
            .with_help_message(StyleSheet::new().with_fg(Color::DarkGrey))
    } else {
        RenderConfig::empty()
            .with_prompt_prefix(Styled::new("?"))
            .with_answered_prompt_prefix(Styled::new("✓"))
            .with_highlighted_option_prefix(Styled::new(">"))
    };
    inquire::set_global_render_config(config);
}

pub fn is_interactive() -> bool {
    INTERACTIVE.load(Ordering::Relaxed)
}

fn running_in_ci() -> bool {
    std::env::var("CI").is_ok_and(|value| !value.is_empty() && value != "0" && !value.eq_ignore_ascii_case("false"))
}

/// One option of a [`select`].
pub struct Choice<T> {
    label: String,
    value: T,
}

impl<T> Choice<T> {
    pub fn new(label: impl Into<String>, value: T) -> Self {
        Self { label: label.into(), value }
    }
}

impl<T> fmt::Display for Choice<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.label)
    }
}

/// Asks to pick one of `choices`, starting on the choice at `cursor`.
///
/// Callers check [`is_interactive`] first and explain which option replaces
/// the question; reaching this without a terminal still fails cleanly.
pub fn select<T>(question: &str, choices: Vec<Choice<T>>, cursor: usize) -> Result<T, CliError> {
    ensure_interactive(question)?;
    let page_size = choices.len().clamp(1, 12);
    Select::new(question, choices)
        .with_help_message(SELECT_HELP)
        .with_starting_cursor(cursor)
        .with_page_size(page_size)
        .without_filtering()
        .prompt()
        .map(|choice| choice.value)
        .map_err(prompt_error)
}

/// Asks a yes/no question.
pub fn confirm(question: &str, default: bool) -> Result<bool, CliError> {
    ensure_interactive(question)?;
    Confirm::new(question).with_default(default).with_help_message(CONFIRM_HELP).prompt().map_err(prompt_error)
}

fn ensure_interactive(question: &str) -> Result<(), CliError> {
    if is_interactive() {
        return Ok(());
    }

    Err(CliError::decision_required("An answer is required, but this terminal is not interactive.")
        .detail("Question", question)
        .hint("Run the command in an interactive terminal, or pass the option that answers this question."))
}

fn prompt_error(error: InquireError) -> CliError {
    match error {
        InquireError::OperationCanceled | InquireError::OperationInterrupted => {
            CliError::cancelled("Cancelled. Nothing was changed.")
        }
        InquireError::NotTTY => CliError::decision_required("An answer is required, but no terminal is available.")
            .hint("Pass the option that answers this question, or run the command in an interactive terminal."),
        error => CliError::failure("The question could not be displayed.").reason(error.to_string()),
    }
}
