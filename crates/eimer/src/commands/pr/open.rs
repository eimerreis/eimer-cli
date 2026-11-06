//! `pr open [id]`: open the PR of the current branch (or a given ID) in the browser.

use anyhow::{Result, bail};
use clap::Args;

use super::shared::{Platform, Target, TargetArgs, Warnings, finish};
use super::{azdo, github};
use crate::core::process::{open_url, run_text};
use crate::core::ui;

#[derive(Args)]
pub struct OpenArgs {
    #[command(flatten)]
    target: TargetArgs,
}

const NOT_FOUND_HINT: &str = "Try `pr list --all` or pass --id to open a specific PR.";

pub fn run(args: OpenArgs) -> Result<()> {
    let result = (|| -> Result<()> {
        let target = args.target.resolve()?;
        match target.platform {
            Platform::Github => open_github(&target),
            Platform::AzureDevops => open_azure(&target),
        }
    })();
    finish(
        result,
        "Failed to open PR",
        "Make sure your browser opener is available and your GitHub or Azure CLI session is authenticated.",
    )
}

fn open_github(target: &Target) -> Result<()> {
    let repo = github::parse_repo(&target.remote_url, target.repo.as_deref())?;
    let mut warnings = Warnings::default();
    let pr = ui::with_spinner("Opening GitHub pull request", || {
        github::find_pr(&repo, target.pr_id, &target.branch, &mut warnings)
    });
    warnings.flush(false);
    let Some(pr) = pr? else {
        ui::print_info(
            &format!("No pull request found for {}.", target.lookup_label()),
            Some(NOT_FOUND_HINT),
        );
        return Ok(());
    };

    let Some(url) = pr.html_url.as_deref().filter(|u| !u.is_empty()) else {
        bail!("Found PR #{} but could not determine a web URL.", pr.number);
    };
    open_url(url)?;
    ui::print_success(&format!("Opened PR #{}: {}", pr.number, pr.title));
    Ok(())
}

fn open_azure(target: &Target) -> Result<()> {
    let mut warnings = Warnings::default();
    let pr = ui::with_spinner("Opening Azure DevOps pull request", || {
        azdo::find_pr(
            target.pr_id,
            &target.branch,
            target.repo.as_deref(),
            &mut warnings,
        )
    });
    warnings.flush(false);
    let Some(pr) = pr? else {
        ui::print_info(
            &format!("No pull request found for {}.", target.lookup_label()),
            Some(NOT_FOUND_HINT),
        );
        return Ok(());
    };

    let id = pr.pull_request_id.to_string();
    run_text(azdo::az(
        &["az", "repos", "pr", "show", "--id", &id, "--open"],
        "none",
    ))?;
    ui::print_success(&format!("Opened PR #{}: {}", pr.pull_request_id, pr.title));
    Ok(())
}
