//! `pr list`: open PRs authored by me (or awaiting my review) in this repo or all repos.

use anyhow::{Context, Result, anyhow};
use clap::Args;
use serde::{Deserialize, Serialize};

use super::azdo::{AzurePullRequest, AzureReviewer, az};
use super::github;
use super::shared::{Platform, argv, detect_platform, origin_url, strip_ref_prefix, timestamp};
use crate::core::azure::{parse_remote, parse_repo_name};
use crate::core::process::run_json;
use crate::core::prompt;
use crate::core::ui::{self, symbols};

#[derive(Args)]
pub struct ListArgs {
    /// Print machine-readable JSON
    #[arg(long, short = 'j')]
    json: bool,
    /// Search across all repos
    #[arg(long, short = 'a')]
    all: bool,
    /// Show PRs where I am a reviewer
    #[arg(long, short = 'r')]
    reviewer: bool,
    /// Maximum number of PRs to fetch
    #[arg(long, short = 't', default_value_t = 50, value_parser = clap::value_parser!(u32).range(1..))]
    top: u32,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ListItem {
    platform: Platform,
    id: u64,
    title: String,
    url: String,
    repository: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    branch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    updated_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    author: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reviewer_vote: Option<i64>,
}

pub fn run(args: ListArgs) -> Result<()> {
    (|| -> Result<()> {
        let remote = origin_url()?;
        let platform = detect_platform(&remote)?;
        // Resolve the Azure identity before the spinner starts: it may prompt.
        let identity = match platform {
            Platform::AzureDevops => Some(resolve_azure_identity()?),
            Platform::Github => None,
        };
        let items = ui::spin_unless(args.json, "Loading pull requests", || match &identity {
            None => list_github(&remote, &args),
            Some(identity) => list_azure(&remote, &args, identity),
        })?;

        if args.json {
            return ui::print_json(&items);
        }
        print_list(&items, args.reviewer, args.all);
        Ok(())
    })()
    .context("Failed to list pull requests")
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GitHubRepoRef {
    name_with_owner: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GitHubPrListItem {
    number: u64,
    #[serde(default)]
    title: String,
    url: Option<String>,
    head_ref_name: Option<String>,
    updated_at: Option<String>,
    created_at: Option<String>,
    repository: Option<GitHubRepoRef>,
}

fn list_github(remote: &str, args: &ListArgs) -> Result<Vec<ListItem>> {
    let top = args.top.to_string();
    let (prs, repo_slug): (Vec<GitHubPrListItem>, Option<String>) = if args.all {
        // Qualifiers must be flags: gh quotes a positional query, so "author:@me" would be plain text.
        let who = if args.reviewer {
            "--review-requested"
        } else {
            "--author"
        };
        let prs = run_json(argv(&[
            "gh",
            "search",
            "prs",
            "--state",
            "open",
            who,
            "@me",
            "--limit",
            &top,
            "--json",
            "number,title,url,updatedAt,repository",
        ]))?;
        (prs, None)
    } else {
        let slug = github::parse_repo(remote, None)?.slug();
        let fields = "number,title,url,headRefName,updatedAt";
        let cmd = if args.reviewer {
            argv(&[
                "gh",
                "pr",
                "list",
                "--repo",
                &slug,
                "--state",
                "open",
                "--search",
                "review-requested:@me",
                "--json",
                fields,
                "--limit",
                &top,
            ])
        } else {
            argv(&[
                "gh", "pr", "list", "--repo", &slug, "--author", "@me", "--state", "open",
                "--json", fields, "--limit", &top,
            ])
        };
        (run_json(cmd)?, Some(slug))
    };

    Ok(prs
        .into_iter()
        .filter_map(|pr| {
            let url = pr.url.filter(|u| !u.is_empty())?;
            let repository = match &repo_slug {
                Some(slug) => slug.clone(),
                None => pr
                    .repository
                    .and_then(|r| r.name_with_owner)
                    .filter(|n| !n.is_empty())
                    .unwrap_or_else(|| "unknown/unknown".into()),
            };
            Some(ListItem {
                platform: Platform::Github,
                id: pr.number,
                title: pr.title,
                url,
                repository,
                branch: pr.head_ref_name,
                updated_at: pr.updated_at.or(pr.created_at),
                author: None,
                reviewer_vote: None,
            })
        })
        .collect())
}

fn resolve_azure_identity() -> Result<String> {
    #[derive(Deserialize)]
    struct User {
        name: Option<String>,
    }
    #[derive(Deserialize)]
    struct Account {
        user: Option<User>,
    }
    let account: Account = run_json(argv(&["az", "account", "show", "--output", "json"]))?;
    if let Some(name) = account
        .user
        .and_then(|u| u.name)
        .map(|n| n.trim().to_string())
        .filter(|n| !n.is_empty())
    {
        return Ok(name);
    }

    let prompted = prompt::text("Azure DevOps user email", None, |_| None).unwrap_or_default();
    if prompted.is_empty() {
        return Err(anyhow!(
            "Missing Azure identity. Re-run with interactive terminal or ensure 'az account show' includes user.name."
        ));
    }
    Ok(prompted)
}

fn matches_identity(candidate: Option<&str>, identity: &str) -> bool {
    candidate.is_some_and(|c| c.to_lowercase() == identity.to_lowercase())
}

fn reviewer_vote(reviewers: &[AzureReviewer], identity: &str) -> i64 {
    reviewers
        .iter()
        .find(|r| matches_identity(r.unique_name.as_deref(), identity))
        .and_then(|r| r.vote)
        .unwrap_or(0)
}

fn azure_pr_url(pr: &AzurePullRequest, base_url: &str) -> String {
    if let Some(web_url) = pr.repo_web_url() {
        return format!("{web_url}/pullrequest/{}", pr.pull_request_id);
    }
    match pr.repo_name() {
        Some(name) if !base_url.is_empty() => {
            format!("{base_url}/_git/{name}/pullrequest/{}", pr.pull_request_id)
        }
        _ => String::new(),
    }
}

fn list_azure(remote: &str, args: &ListArgs, identity: &str) -> Result<Vec<ListItem>> {
    let repo_name =
        if args.all {
            None
        } else {
            Some(parse_repo_name(remote).ok_or_else(|| {
                anyhow!("Could not parse repository name from remote URL: {remote}")
            })?)
        };
    let base_url = parse_remote(remote).map(|p| p.base_url).unwrap_or_default();

    let top = args.top.to_string();
    let mut cmd = az(
        &[
            "az", "repos", "pr", "list", "--status", "active", "--top", &top,
        ],
        "json",
    );
    if let Some(repo) = &repo_name {
        cmd.extend(["--repository".into(), repo.clone()]);
    }
    let prs: Vec<AzurePullRequest> = run_json(&cmd)?;

    let mut filtered: Vec<AzurePullRequest> = prs
        .into_iter()
        .filter(|pr| {
            if args.reviewer {
                pr.reviewers
                    .iter()
                    .flatten()
                    .any(|r| matches_identity(r.unique_name.as_deref(), identity))
            } else {
                matches_identity(
                    pr.created_by
                        .as_ref()
                        .and_then(|c| c.unique_name.as_deref()),
                    identity,
                )
            }
        })
        .collect();
    filtered.sort_by_key(|pr| std::cmp::Reverse(timestamp(pr.creation_date.as_deref())));

    Ok(filtered
        .into_iter()
        .filter_map(|pr| {
            let url = azure_pr_url(&pr, &base_url);
            if url.is_empty() {
                return None;
            }
            let author = pr.created_by.as_ref().and_then(|c| {
                c.display_name
                    .clone()
                    .filter(|n| !n.is_empty())
                    .or_else(|| c.unique_name.clone())
            });
            Some(ListItem {
                platform: Platform::AzureDevops,
                id: pr.pull_request_id,
                url,
                repository: pr
                    .repo_name()
                    .map(String::from)
                    .or_else(|| repo_name.clone())
                    .unwrap_or_else(|| "unknown".into()),
                branch: Some(strip_ref_prefix(pr.source_ref_name.as_deref())),
                updated_at: pr.creation_date.clone().or_else(|| pr.closed_date.clone()),
                author,
                reviewer_vote: args
                    .reviewer
                    .then(|| reviewer_vote(pr.reviewers.as_deref().unwrap_or_default(), identity)),
                title: pr.title,
            })
        })
        .collect())
}

fn reviewer_vote_label(vote: Option<i64>) -> &'static str {
    match vote {
        Some(10) => "APPROVED",
        Some(5) => "APPROVED-WITH-SUGGESTIONS",
        Some(-5) => "WAITING-FOR-AUTHOR",
        Some(-10) => "REJECTED",
        _ => "NO-VOTE",
    }
}

fn print_list(items: &[ListItem], reviewer: bool, all: bool) {
    if items.is_empty() {
        let mode = if reviewer { "reviewer" } else { "author" };
        let scope = if all { "all repos" } else { "current repo" };
        ui::print_info(
            &format!("No open PRs found for {mode} mode in {scope}."),
            (!all).then_some("Try --all to search across all repos."),
        );
        return;
    }

    let rows = items.iter().map(|item| {
        let status = if reviewer {
            format!(
                "{} {}",
                symbols::pending(),
                reviewer_vote_label(item.reviewer_vote)
            )
        } else {
            let label = match item.platform {
                Platform::Github => "GITHUB",
                Platform::AzureDevops => "AZDO",
            };
            format!("{} {label}", symbols::info())
        };
        vec![
            status,
            ui::terminal_link(&format!("#{}", item.id), &item.url),
            item.title.clone(),
            item.repository.clone(),
            item.branch
                .clone()
                .filter(|b| !b.is_empty())
                .unwrap_or_else(|| "-".into()),
            match item.updated_at.as_deref() {
                Some(at) if !at.is_empty() => ui::relative_time(Some(at)),
                _ => "-".into(),
            },
        ]
    });
    println!(
        "{}",
        ui::render_table(["State", "PR", "Title", "Repo", "Branch", "Updated"], rows)
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vote_labels() {
        assert_eq!(reviewer_vote_label(Some(10)), "APPROVED");
        assert_eq!(reviewer_vote_label(Some(5)), "APPROVED-WITH-SUGGESTIONS");
        assert_eq!(reviewer_vote_label(Some(-5)), "WAITING-FOR-AUTHOR");
        assert_eq!(reviewer_vote_label(Some(-10)), "REJECTED");
        assert_eq!(reviewer_vote_label(Some(0)), "NO-VOTE");
        assert_eq!(reviewer_vote_label(None), "NO-VOTE");
    }

    #[test]
    fn identity_and_urls() {
        assert!(matches_identity(Some("Me@X.com"), "me@x.com"));
        assert!(!matches_identity(None, "me@x.com"));
        let reviewers: Vec<AzureReviewer> = serde_json::from_str(
            r#"[{"uniqueName": "a@x.com", "vote": 5}, {"uniqueName": "me@x.com", "vote": -5}]"#,
        )
        .unwrap();
        assert_eq!(reviewer_vote(&reviewers, "ME@x.com"), -5);
        assert_eq!(reviewer_vote(&reviewers, "z@x.com"), 0);

        let pr: AzurePullRequest = serde_json::from_str(
            r#"{"pullRequestId": 3, "title": "t", "repository": {"name": "repo"}}"#,
        )
        .unwrap();
        assert_eq!(
            azure_pr_url(&pr, "https://dev.azure.com/o/p"),
            "https://dev.azure.com/o/p/_git/repo/pullrequest/3"
        );
        assert_eq!(azure_pr_url(&pr, ""), "");
        let pr: AzurePullRequest = serde_json::from_str(
            r#"{"pullRequestId": 3, "title": "t", "repository": {"name": "repo", "webUrl": "https://w/_git/repo"}}"#,
        )
        .unwrap();
        assert_eq!(azure_pr_url(&pr, ""), "https://w/_git/repo/pullrequest/3");
    }
}
