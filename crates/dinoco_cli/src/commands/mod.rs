pub mod init;
pub mod migrate;
pub mod models;
mod review;
pub mod seed;

/// `dinoco <args>`, with `--workspace` when one is selected: the command line
/// that hints and next steps tell the user to run.
pub(crate) fn command_line(args: &str, workspace: Option<&str>) -> String {
    match workspace {
        Some(workspace) => format!("dinoco {args} --workspace {workspace}"),
        None => format!("dinoco {args}"),
    }
}
