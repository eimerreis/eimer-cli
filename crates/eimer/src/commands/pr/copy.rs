//! `pr copy [id]`: copy a Markdown link to the PR to the clipboard.

use anyhow::{Context, Result, anyhow};
use clap::Args;
use serde::Serialize;

use super::shared::{Platform, Target, TargetArgs, Warnings};
use super::{azdo, github};
use crate::core::azure::{parse_remote, parse_repo_name};
use crate::core::process::{NoClipboard, copy_to_clipboard};
use crate::core::ui;

#[derive(Args)]
pub struct CopyArgs {
    #[command(flatten)]
    target: TargetArgs,
    /// Print machine-readable JSON
    #[arg(long, short = 'j')]
    json: bool,
}

#[derive(Debug, Serialize)]
struct CopyTarget {
    id: u64,
    title: String,
    url: String,
    platform: Platform,
}

#[derive(Serialize)]
struct CopyOutput<'a> {
    copied: bool,
    text: &'a str,
    pr: &'a CopyTarget,
}

pub fn run(args: CopyArgs) -> Result<()> {
    (|| -> Result<()> {
        let target = args.target.resolve()?;
        let mut warnings = Warnings::default();
        let resolved = ui::spin_unless(args.json, "Resolving pull request", || {
            match target.platform {
                Platform::Github => resolve_github(&target, &mut warnings),
                Platform::AzureDevops => resolve_azure(&target, &mut warnings),
            }
        });
        warnings.flush(args.json);
        let Some(pr) = resolved? else {
            ui::print_info(
                &format!("No pull request found for {}.", target.lookup_label()),
                None,
            );
            return Ok(());
        };

        let markdown = markdown_link(&pr);
        let copied = match ui::spin_unless(args.json, "Copying pull request link", || {
            copy_to_clipboard(&markdown)
        }) {
            Ok(_) => true,
            // Headless/SSH: still give the user the link instead of failing.
            Err(err) if err.downcast_ref::<NoClipboard>().is_some() => {
                if !args.json {
                    ui::print_warning(&err.to_string(), None);
                }
                false
            }
            Err(err) => return Err(err),
        };

        if args.json {
            return ui::print_json(&CopyOutput {
                copied,
                text: &markdown,
                pr: &pr,
            });
        }
        if copied {
            ui::print_success(&format!("Copied to clipboard: {markdown}"));
        } else {
            println!("{markdown}");
        }
        Ok(())
    })()
    .context("Failed to copy PR link")
}

fn markdown_link(pr: &CopyTarget) -> String {
    format!("[Pull Request {}: {}]({})", pr.id, pr.title, pr.url)
}

fn resolve_github(target: &Target, warnings: &mut Warnings) -> Result<Option<CopyTarget>> {
    let repo = github::parse_repo(&target.remote_url, target.repo.as_deref())?;
    let pr = github::find_pr(&repo, target.pr_id, &target.branch, warnings)?;
    Ok(pr.and_then(|pr| {
        let url = pr.html_url.filter(|u| !u.is_empty())?;
        Some(CopyTarget {
            id: pr.number,
            title: pr.title,
            url,
            platform: Platform::Github,
        })
    }))
}

fn resolve_azure(target: &Target, warnings: &mut Warnings) -> Result<Option<CopyTarget>> {
    let Some(pr) = azdo::find_pr(
        target.pr_id,
        &target.branch,
        target.repo.as_deref(),
        warnings,
    )?
    else {
        return Ok(None);
    };

    let repo_name = target
        .repo
        .clone()
        .or_else(|| pr.repo_name().map(String::from))
        .or_else(|| parse_repo_name(&target.remote_url))
        .ok_or_else(|| {
            anyhow!(
                "Could not parse repository name from remote URL: {}",
                target.remote_url
            )
        })?;
    let base_url = parse_remote(&target.remote_url)
        .map(|p| p.base_url)
        .ok_or_else(|| anyhow!("Could not determine Azure DevOps project URL from git remote."))?;

    Ok(Some(CopyTarget {
        id: pr.pull_request_id,
        url: format!(
            "{base_url}/_git/{repo_name}/pullrequest/{}",
            pr.pull_request_id
        ),
        title: pr.title,
        platform: Platform::AzureDevops,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_markdown_and_json() {
        let pr = CopyTarget {
            id: 42,
            title: "feat: x".into(),
            url: "https://u/42".into(),
            platform: Platform::AzureDevops,
        };
        assert_eq!(
            markdown_link(&pr),
            "[Pull Request 42: feat: x](https://u/42)"
        );
        assert_eq!(
            serde_json::to_value(&pr).unwrap(),
            serde_json::json!({"id": 42, "title": "feat: x", "url": "https://u/42", "platform": "azure-devops"})
        );
    }
}
