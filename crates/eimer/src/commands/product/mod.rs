//! `eimer product`: turn Changesets changelogs into public update posts.
//!
//! `update` picks the changes of one feature, writes a draft post
//! (`<updatesDir>/<date>-<slug>.<lang>.md`) and copies a prompt for a coding agent, which
//! writes the copy, captures media and opens a PR. Posts list the change ids they cover,
//! so the repo itself records what is published. See docs/features/009-product-updates.md.

mod changelog;
mod changes;
mod check;
mod config;
mod frontmatter;
mod glob;
mod init;
mod posts;
mod status;
mod template;
mod update;

use anyhow::Result;
use clap::{Args, Subcommand};

#[derive(Args)]
pub struct ProductArgs {
    #[command(subcommand)]
    command: ProductCommand,
}

#[derive(Subcommand)]
enum ProductCommand {
    /// Pick changes for an update post, write the draft and copy the agent prompt
    Update(update::UpdateArgs),
    /// List changes not covered by an update post yet
    Status(status::StatusArgs),
    /// Validate update posts (exit 1 on errors)
    Check(check::CheckArgs),
    /// Write .eimer/product.json for this repo
    Init(init::InitArgs),
}

pub fn run(args: ProductArgs) -> Result<()> {
    match args.command {
        ProductCommand::Update(a) => update::run(a),
        ProductCommand::Status(a) => status::run(a),
        ProductCommand::Check(a) => check::run(a),
        ProductCommand::Init(a) => init::run(a),
    }
}
