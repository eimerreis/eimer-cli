//! `eimer release`: release changelogs, approvals and config (port of `@tapio/release`).
//!
//! Talks to the Azure DevOps REST API directly so it also runs in CI without `az`
//! (auth via `SYSTEM_ACCESSTOKEN` / `AZURE_DEVOPS_PAT`). Config is shared with the TS CLI
//! at `~/.config/tapio-release/config.json`.

mod api;
mod approve;
mod areas;
mod changelog;
mod changelog_format;
mod client;
mod config;
mod configure;
mod prod_stage;
mod runs;
mod teams_review;
mod teams_webhook;
#[cfg(test)]
mod test_server;
mod util;

use anyhow::Result;
use clap::{Args, Subcommand};

#[derive(Args)]
pub struct ReleaseArgs {
    #[command(subcommand)]
    command: ReleaseCommand,
}

#[derive(Subcommand)]
enum ReleaseCommand {
    /// Build release changelog from pipeline commits on a branch
    Changelog(changelog::ChangelogArgs),
    /// Approve pending Azure DevOps pipeline approvals
    Approve(approve::ApproveArgs),
    /// Configure release defaults and Teams channels
    Configure(configure::ConfigureArgs),
}

pub fn run(args: ReleaseArgs) -> Result<()> {
    match args.command {
        ReleaseCommand::Changelog(a) => changelog::run(a),
        ReleaseCommand::Approve(a) => approve::run(a),
        ReleaseCommand::Configure(a) => configure::run(a),
    }
}
