//! `eimer pr`: pull request workflows for Azure DevOps (`az repos`) and GitHub (`gh`).

mod azdo;
mod comments;
mod copy;
mod create;
mod github;
mod list;
mod open;
mod shared;
mod show;
mod show_helpers;
mod threads;

use anyhow::Result;
use clap::{Args, Subcommand};

#[derive(Args)]
pub struct PrArgs {
    #[command(subcommand)]
    command: PrCommand,
}

#[derive(Subcommand)]
enum PrCommand {
    /// Create a pull request (with auto-complete on)
    Create(create::CreateArgs),
    /// List pull request review comments
    Comments(comments::CommentsArgs),
    /// Copy PR reference with link to clipboard
    Copy(copy::CopyArgs),
    /// List open pull requests
    List(list::ListArgs),
    /// Open pull request for current branch
    Open(open::OpenArgs),
    /// Show PR merge readiness details
    Show(show::ShowArgs),
}

pub fn run(args: PrArgs) -> Result<()> {
    match args.command {
        PrCommand::Create(a) => create::run(a),
        PrCommand::Comments(a) => comments::run(a),
        PrCommand::Copy(a) => copy::run(a),
        PrCommand::List(a) => list::run(a),
        PrCommand::Open(a) => open::run(a),
        PrCommand::Show(a) => show::run(a),
    }
}
