//! `pr comments [id]`: list review comment threads of a PR.

use anyhow::Result;
use clap::Args;

use super::shared::{Platform, TargetArgs, Warnings, finish};
use super::{azdo, github, threads};
use crate::core::ui;

#[derive(Args)]
pub struct CommentsArgs {
    #[command(flatten)]
    target: TargetArgs,
    /// Print machine-readable JSON
    #[arg(long, short = 'j')]
    json: bool,
}

pub fn run(args: CommentsArgs) -> Result<()> {
    let result = (|| -> Result<()> {
        let target = args.target.resolve()?;
        let mut warnings = Warnings::default();
        let result = ui::spin_unless(args.json, "Loading review comments", || {
            match target.platform {
                Platform::Github => github::load_comments(
                    &target.remote_url,
                    &target.branch,
                    target.pr_id,
                    target.repo.as_deref(),
                    &mut warnings,
                ),
                Platform::AzureDevops => azdo::load_comments(
                    &target.branch,
                    target.pr_id,
                    target.repo.as_deref(),
                    &mut warnings,
                ),
            }
        });
        warnings.flush(args.json);
        let result = result?;

        if args.json {
            return ui::print_json(&result);
        }
        threads::print_human_readable(&result);
        Ok(())
    })();
    finish(
        result,
        "Failed to list PR comments",
        "Check your PR lookup inputs and make sure `gh` or `az` is authenticated.",
    )
}
