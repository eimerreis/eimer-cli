//! `pr create [title...]`: push the branch if needed, create an Azure DevOps PR,
//! enable squash auto-complete and open it in the browser.

use anyhow::{Context, Result, anyhow, bail};
use clap::Args;
use serde::Deserialize;

use super::azdo::az;
use super::shared::required;
use crate::core::azure::parse_repo_name;
use crate::core::error::HintExt;
use crate::core::process::{open_url, run_json, run_text};
use crate::core::{prompt, ui};

#[derive(Args)]
pub struct CreateArgs {
    /// PR title (prompted when omitted)
    #[arg(trailing_var_arg = true, value_name = "TITLE")]
    title: Vec<String>,
}

#[derive(Deserialize)]
struct WebLink {
    href: Option<String>,
}

#[derive(Deserialize)]
struct Links {
    web: Option<WebLink>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreatedPullRequest {
    pull_request_id: u64,
    #[serde(rename = "_links")]
    links: Option<Links>,
}

pub fn run(args: CreateArgs) -> Result<()> {
    let mut title = args.title.join(" ").trim().to_string();
    if title.is_empty() {
        title = prompt::text("PR title", None, required("Title is required")).unwrap_or_default();
        if title.is_empty() {
            bail!("Missing PR title. Usage: pr create \"your title\"");
        }
    }
    create(&title).context("Failed to create PR")
}

/// Merge commit message with quote characters removed.
fn merge_commit_message(id: u64, title: &str) -> String {
    format!("Merged PR {id}: {title}").replace(['"', '\''], "")
}

fn create(title: &str) -> Result<()> {
    let mut branch = run_text(["git", "branch", "--show-current"])?
        .trim()
        .to_string();
    if branch.is_empty() {
        branch =
            prompt::text("Branch name", None, required("Branch is required")).unwrap_or_default();
        if branch.is_empty() {
            bail!("Could not determine current git branch.");
        }
    }

    let remote = run_text(["git", "remote", "get-url", "origin"])?
        .trim()
        .to_string();
    if remote.to_lowercase().contains("github.com") {
        return Err(anyhow!("`pr create` only supports Azure DevOps remotes"))
            .hint("Use `gh pr create` for GitHub repositories.");
    }
    let repo = parse_repo_name(&remote)
        .ok_or_else(|| anyhow!("Could not parse repository name from remote URL: {remote}"))?;

    let pushed = ui::with_spinner("Ensuring remote branch exists", || {
        ensure_remote_branch(&branch)
    })?;
    if pushed {
        ui::print_info(&format!("Pushed branch '{branch}' to origin."), None);
    }

    let pr: CreatedPullRequest = ui::with_spinner("Creating pull request", || {
        run_json(az(
            &[
                "az",
                "repos",
                "pr",
                "create",
                "--repository",
                &repo,
                "--source-branch",
                &branch,
                "--title",
                title,
            ],
            "json",
        ))
    })?;
    let id = pr.pull_request_id.to_string();

    let message = merge_commit_message(pr.pull_request_id, title);
    ui::with_spinner("Configuring auto-complete", || {
        run_text(az(
            &[
                "az",
                "repos",
                "pr",
                "update",
                "--id",
                &id,
                "--merge-commit-message",
                &message,
                "--auto-complete",
                "true",
                "--squash",
            ],
            "json",
        ))
    })?;

    let web_url = pr
        .links
        .and_then(|l| l.web)
        .and_then(|w| w.href)
        .filter(|h| !h.is_empty());
    match web_url {
        Some(url) => open_url(&url)?,
        None => {
            run_text(az(
                &["az", "repos", "pr", "show", "--id", &id, "--open"],
                "none",
            ))?;
        }
    }

    ui::print_success(&format!("Created PR #{id} for {repo} on {branch}"));
    Ok(())
}

/// Pushes `branch` to origin unless it already exists there. Returns whether it pushed.
fn ensure_remote_branch(branch: &str) -> Result<bool> {
    let remote_branch = run_text(["git", "ls-remote", "--heads", "origin", branch])?;
    if !remote_branch.trim().is_empty() {
        return Ok(false);
    }
    run_text(["git", "push", "-u", "origin", branch])?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_quotes_from_merge_message() {
        assert_eq!(
            merge_commit_message(7, r#"fix: "quoted" isn't 'ok'"#),
            "Merged PR 7: fix: quoted isnt ok"
        );
    }
}
