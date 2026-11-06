//! eimer: developer CLI for Azure DevOps and GitHub workflows.
//!
//! Multi-call binary: when invoked through a symlink named `task`, `pr`, `pipeline`
//! or `release`, it behaves like `eimer <name> ...`.

mod commands;
mod core;

use std::ffi::OsString;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

/// Command groups that can be invoked directly via a symlink of the same name.
pub const MULTICALL_GROUPS: &[&str] = &["task", "pr", "pipeline", "release"];

#[derive(Parser)]
#[command(
    name = "eimer",
    version,
    about = "Developer CLI for Azure DevOps and GitHub workflows",
    propagate_version = true
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Azure DevOps task workflows
    Task(commands::task::TaskArgs),
    /// Pull request workflows (Azure DevOps and GitHub)
    Pr(commands::pr::PrArgs),
    /// Azure DevOps pipeline workflows
    Pipeline(commands::pipeline::PipelineArgs),
    /// Release changelogs and approvals
    Release(commands::release::ReleaseArgs),
    /// Manage shared CLI defaults (~/.config/eimer/config.json)
    Configure(commands::configure::ConfigureArgs),
    /// Update eimer to the latest release
    Upgrade(commands::upgrade::UpgradeArgs),
    /// Create `task`, `pr`, `pipeline`, `release` symlinks next to the eimer binary
    Link(commands::upgrade::LinkArgs),
}

/// Rewrites `task list` (invoked via a `task` symlink) to `eimer task list`.
fn multicall_args(args: Vec<OsString>) -> Vec<OsString> {
    let Some(first) = args.first() else {
        return args;
    };
    // Split on both separators so Windows-style paths work everywhere.
    let raw = first.to_string_lossy();
    let file = raw.rsplit(['/', '\\']).next().unwrap_or_default();
    let invoked = file.strip_suffix(".exe").unwrap_or(file).to_string();
    if MULTICALL_GROUPS.contains(&invoked.as_str()) {
        let mut rewritten = vec![OsString::from("eimer"), OsString::from(invoked)];
        rewritten.extend(args.into_iter().skip(1));
        rewritten
    } else {
        args
    }
}

/// Prompts (dialoguer) hide the cursor while a selection list is shown. If Ctrl-C kills
/// the process mid-prompt, restore the cursor so the user's shell isn't left without one.
fn install_interrupt_handler() {
    let _ = ctrlc::set_handler(|| core::prompt::exit_cancelled());
}

fn main() -> ExitCode {
    install_interrupt_handler();
    let cli = Cli::parse_from(multicall_args(std::env::args_os().collect()));

    let result = match cli.command {
        Commands::Task(args) => commands::task::run(args),
        Commands::Pr(args) => commands::pr::run(args),
        Commands::Pipeline(args) => commands::pipeline::run(args),
        Commands::Release(args) => commands::release::run(args),
        Commands::Configure(args) => commands::configure::run(args),
        Commands::Upgrade(args) => commands::upgrade::run(args),
        Commands::Link(args) => commands::upgrade::link(args),
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => ExitCode::from(core::error::report(&err).clamp(0, 255) as u8),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn os(args: &[&str]) -> Vec<OsString> {
        args.iter().map(OsString::from).collect()
    }

    #[test]
    fn rewrites_symlink_invocations() {
        assert_eq!(
            multicall_args(os(&["/usr/local/bin/task", "list"])),
            os(&["eimer", "task", "list"])
        );
        assert_eq!(
            multicall_args(os(&["C:\\bin\\pr.exe", "show", "1"])),
            os(&["eimer", "pr", "show", "1"])
        );
        assert_eq!(
            multicall_args(os(&["eimer", "task"])),
            os(&["eimer", "task"])
        );
    }

    #[test]
    fn cli_definition_is_valid() {
        use clap::CommandFactory;
        Cli::command().debug_assert();
    }
}
