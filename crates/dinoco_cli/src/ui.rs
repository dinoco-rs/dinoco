//! Terminal presentation shared by every command.
//!
//! Output follows a small vocabulary: section headings, aligned key/value
//! fields, change lines (`+`, `~`, `-`), and `warning:`/`error:`/`help:`
//! diagnostics. Results go to standard output; warnings, errors and progress
//! go to standard error. Color is used only when the destination stream is a
//! terminal, `NO_COLOR` is unset and `--no-color` was not passed.

use std::io::IsTerminal;
use std::sync::atomic::{AtomicBool, Ordering};

use colored::{ColoredString, Colorize};

static STDOUT_COLOR: AtomicBool = AtomicBool::new(false);
static STDERR_COLOR: AtomicBool = AtomicBool::new(false);
/// Whether anything was written yet, and whether the last line was blank,
/// so sections and error reports are separated by exactly one blank line.
static WROTE_OUTPUT: AtomicBool = AtomicBool::new(false);
static LAST_LINE_BLANK: AtomicBool = AtomicBool::new(false);

fn out(line: impl AsRef<str>) {
    let line = line.as_ref();
    WROTE_OUTPUT.store(true, Ordering::Relaxed);
    LAST_LINE_BLANK.store(line.is_empty(), Ordering::Relaxed);
    println!("{line}");
}

fn needs_separator() -> bool {
    WROTE_OUTPUT.load(Ordering::Relaxed) && !LAST_LINE_BLANK.load(Ordering::Relaxed)
}

/// Starts an error report: separated from earlier output by a blank line.
pub(crate) fn begin_error_report() {
    if needs_separator() {
        eprintln!();
    }
    WROTE_OUTPUT.store(true, Ordering::Relaxed);
    LAST_LINE_BLANK.store(false, Ordering::Relaxed);
}

/// Decides, once at startup, whether each stream gets ANSI colors.
pub fn configure(no_color: bool) {
    let allowed = !no_color && std::env::var_os("NO_COLOR").is_none_or(|value| value.is_empty());
    let forced = std::env::var_os("CLICOLOR_FORCE").is_some_and(|value| !value.is_empty() && value != "0");
    STDOUT_COLOR.store(allowed && (forced || std::io::stdout().is_terminal()), Ordering::Relaxed);
    STDERR_COLOR.store(allowed && (forced || std::io::stderr().is_terminal()), Ordering::Relaxed);
    // Styling is applied per stream below, so the crate itself never decides.
    colored::control::set_override(true);
}

/// Whether interactive prompts (drawn on standard error) may use colors.
pub fn prompt_colors() -> bool {
    STDERR_COLOR.load(Ordering::Relaxed)
}

#[derive(Clone, Copy)]
enum Stream {
    Out,
    Err,
}

fn styled(stream: Stream, text: &str, style: impl FnOnce(&str) -> ColoredString) -> String {
    let enabled = match stream {
        Stream::Out => STDOUT_COLOR.load(Ordering::Relaxed),
        Stream::Err => STDERR_COLOR.load(Ordering::Relaxed),
    };
    if enabled { style(text).to_string() } else { text.to_string() }
}

/// A section title, separated from the previous output by a blank line.
pub fn heading(title: impl AsRef<str>) {
    if needs_separator() {
        out("");
    }
    out(styled(Stream::Out, title.as_ref(), |text| text.bold()));
}

/// Aligned `label  value` lines under a heading.
pub fn fields<L: AsRef<str>, V: AsRef<str>>(fields: &[(L, V)]) {
    let width = fields.iter().map(|(label, _)| label.as_ref().len()).max().unwrap_or(0);
    for (label, value) in fields {
        let label = format!("{:<width$}", label.as_ref());
        out(format!("  {}  {}", styled(Stream::Out, &label, |text| text.dimmed()), value.as_ref()));
    }
}

/// An indented line of regular output.
pub fn line(message: impl AsRef<str>) {
    out(format!("  {}", message.as_ref()));
}

/// A plain paragraph of regular output.
pub fn text(message: impl AsRef<str>) {
    out(message);
}

pub fn blank() {
    out("");
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Change {
    Add,
    Modify,
    Remove,
}

/// One planned change: `+` additions, `~` modifications, `-` removals.
pub fn change(kind: Change, description: impl AsRef<str>) {
    let marker = match kind {
        Change::Add => styled(Stream::Out, "+", |text| text.green().bold()),
        Change::Modify => styled(Stream::Out, "~", |text| text.yellow().bold()),
        Change::Remove => styled(Stream::Out, "-", |text| text.red().bold()),
    };
    out(format!("  {marker} {}", description.as_ref()));
}

/// A data-loss or data-rewrite consequence of a planned change.
pub fn impact(destructive: bool, message: impl AsRef<str>) {
    let marker = if destructive {
        styled(Stream::Out, "!", |text| text.red().bold())
    } else {
        styled(Stream::Out, "!", |text| text.yellow().bold())
    };
    out(format!("  {marker} {}", message.as_ref()));
}

/// A completed step.
pub fn success(message: impl AsRef<str>) {
    out(format!("{} {}", styled(Stream::Out, "✓", |text| text.green().bold()), message.as_ref()));
}

/// Neutral status information, such as "No schema changes were found."
pub fn info(message: impl AsRef<str>) {
    out(message);
}

/// Work in progress, on standard error so standard output stays clean.
pub fn progress(message: impl AsRef<str>) {
    WROTE_OUTPUT.store(true, Ordering::Relaxed);
    LAST_LINE_BLANK.store(false, Ordering::Relaxed);
    eprintln!("{}", styled(Stream::Err, message.as_ref(), |text| text.dimmed()));
}

pub fn warning(message: impl AsRef<str>) {
    WROTE_OUTPUT.store(true, Ordering::Relaxed);
    LAST_LINE_BLANK.store(false, Ordering::Relaxed);
    eprintln!("{} {}", styled(Stream::Err, "warning:", |text| text.yellow().bold()), message.as_ref());
}

pub fn error(message: impl AsRef<str>) {
    eprintln!("{} {}", styled(Stream::Err, "error:", |text| text.red().bold()), message.as_ref());
}

/// Error details, aligned like [`fields`], on standard error.
pub fn error_fields<L: AsRef<str>, V: AsRef<str>>(fields: &[(L, V)]) {
    let width = fields.iter().map(|(label, _)| label.as_ref().len() + 1).max().unwrap_or(0);
    for (label, value) in fields {
        let label = format!("{:<width$}", format!("{}:", label.as_ref()));
        eprintln!("  {}  {}", styled(Stream::Err, &label, |text| text.dimmed()), value.as_ref());
    }
}

/// A paragraph of error explanation on standard error.
pub fn error_text(message: impl AsRef<str>) {
    eprintln!("{}", message.as_ref());
}

/// What the user can do next.
pub fn help(message: impl AsRef<str>) {
    eprintln!("{} {}", styled(Stream::Err, "help:", |text| text.cyan().bold()), message.as_ref());
}

pub fn docs(path: &str) {
    let url = format!("https://dinoco.io{path}");
    out("");
    out(format!("Documentation: {}", styled(Stream::Out, &url, |text| text.cyan().underline())));
}

/// `count` followed by `singular`, or `plural` when the count is not one.
pub fn plural(count: usize, singular: &str, plural: &str) -> String {
    format!("{count} {}", if count == 1 { singular } else { plural })
}
