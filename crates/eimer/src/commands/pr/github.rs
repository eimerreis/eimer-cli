//! GitHub PR lookups (`gh pr list`, `gh api`) and review comment thread loading.

use std::collections::HashMap;

use anyhow::{Context, Result, anyhow, bail};
use serde::Deserialize;

use super::shared::{Platform, Warnings, argv, timestamp};
use super::threads::{NormalizedComment, NormalizedResult, NormalizedThread, ResultPr};
use crate::core::process::{run_json, run_text};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitHubRepo {
    pub owner: String,
    pub name: String,
}

impl GitHubRepo {
    pub fn slug(&self) -> String {
        format!("{}/{}", self.owner, self.name)
    }
}

/// `owner/repo` override (extra segments are ignored, like the TS split).
pub fn parse_repo_override(value: &str) -> Result<GitHubRepo> {
    let mut parts = value.split('/');
    match (parts.next(), parts.next()) {
        (Some(owner), Some(name)) if !owner.is_empty() && !name.is_empty() => Ok(GitHubRepo {
            owner: owner.into(),
            name: name.into(),
        }),
        _ => bail!("GitHub repo override must be in the format owner/repo"),
    }
}

/// Parses `https://github.com/o/r(.git)` and `git@github.com:o/r(.git)`.
pub fn parse_repo_from_remote(remote: &str) -> Option<GitHubRepo> {
    let idx = remote.to_ascii_lowercase().find("github.com")?;
    let rest = &remote[idx + "github.com".len()..];
    let rest = rest.strip_prefix(':').or_else(|| rest.strip_prefix('/'))?;
    let (owner, repo) = rest.split_once('/')?;
    let repo = if repo.to_ascii_lowercase().ends_with(".git") {
        &repo[..repo.len() - 4]
    } else {
        repo
    };
    if owner.is_empty()
        || repo.is_empty()
        || repo.contains('/')
        || repo.contains(char::is_whitespace)
    {
        return None;
    }
    Some(GitHubRepo {
        owner: owner.into(),
        name: repo.into(),
    })
}

pub fn parse_repo(remote: &str, repo_override: Option<&str>) -> Result<GitHubRepo> {
    match repo_override {
        Some(value) => parse_repo_override(value),
        None => parse_repo_from_remote(remote)
            .ok_or_else(|| anyhow!("Could not parse GitHub owner/repo from remote: {remote}")),
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct GitHubPullRequest {
    pub number: u64,
    #[serde(default)]
    pub title: String,
    /// Web URL (`html_url` from `gh api`; mapped from `url` for `gh pr list`).
    pub html_url: Option<String>,
}

pub fn find_pr_by_id(repo: &GitHubRepo, id: u64) -> Option<GitHubPullRequest> {
    fetch_pr(repo, id).ok()
}

fn fetch_pr(repo: &GitHubRepo, id: u64) -> Result<GitHubPullRequest> {
    run_json(argv(&[
        "gh",
        "api",
        &format!("repos/{}/pulls/{id}", repo.slug()),
    ]))
}

/// First open PR whose head is `branch`; `None` when there is none.
pub fn find_pr_by_branch(
    repo: &GitHubRepo,
    branch: &str,
    warnings: &mut Warnings,
) -> Result<Option<GitHubPullRequest>> {
    #[derive(Deserialize)]
    struct ListItem {
        number: u64,
        #[serde(default)]
        title: String,
        url: Option<String>,
    }
    let prs: Vec<ListItem> = run_json(argv(&[
        "gh",
        "pr",
        "list",
        "--repo",
        &repo.slug(),
        "--head",
        branch,
        "--json",
        "number,title,updatedAt,url",
        "--limit",
        "20",
    ]))?;
    if prs.len() > 1 {
        warnings.push(format!(
            "Found {} PRs for branch '{branch}'. Using the first result (#{}).",
            prs.len(),
            prs[0].number
        ));
    }
    Ok(prs.into_iter().next().map(|pr| GitHubPullRequest {
        number: pr.number,
        title: pr.title,
        html_url: pr.url,
    }))
}

pub fn find_pr(
    repo: &GitHubRepo,
    id: Option<u64>,
    branch: &str,
    warnings: &mut Warnings,
) -> Result<Option<GitHubPullRequest>> {
    match id {
        Some(id) => Ok(find_pr_by_id(repo, id)),
        None => find_pr_by_branch(repo, branch, warnings),
    }
}

/// `pr comments` for GitHub: by id (errors propagate) or branch (error when none found).
pub fn load_comments(
    remote: &str,
    branch: &str,
    id: Option<u64>,
    repo_override: Option<&str>,
    warnings: &mut Warnings,
) -> Result<NormalizedResult> {
    let repo = parse_repo(remote, repo_override)?;
    let pr = match id {
        Some(id) => fetch_pr(&repo, id)?,
        None => find_pr_by_branch(&repo, branch, warnings)?.ok_or_else(|| {
            anyhow!(
                "No active GitHub PR found for branch '{branch}'. Use --id to target a PR directly."
            )
        })?,
    };
    load_comments_for(&repo, &pr, branch)
}

/// Parses `gh api --paginate` output: zero or more JSON arrays, concatenated.
fn parse_paginated<T: serde::de::DeserializeOwned>(raw: &str) -> Result<Vec<Vec<T>>> {
    serde_json::Deserializer::from_str(raw)
        .into_iter::<Vec<T>>()
        .collect::<std::result::Result<_, _>>()
        .context("Failed to parse paginated GitHub API response")
}

/// Loads and threads the review comments of an already-resolved PR.
pub fn load_comments_for(
    repo: &GitHubRepo,
    pr: &GitHubPullRequest,
    branch: &str,
) -> Result<NormalizedResult> {
    // `--paginate` prints one JSON array per page back to back. `--slurp` would merge them
    // but needs gh >= 2.48, so parse the concatenated pages ourselves.
    let raw = run_text(argv(&[
        "gh",
        "api",
        &format!(
            "repos/{}/pulls/{}/comments?per_page=100",
            repo.slug(),
            pr.number
        ),
        "--paginate",
    ]))?;
    let pages = parse_paginated::<GitHubReviewComment>(&raw)?;
    Ok(NormalizedResult {
        platform: Platform::Github,
        pr: ResultPr {
            id: pr.number,
            title: pr.title.clone(),
            branch: Some(branch.to_string()),
            repository: Some(repo.slug()),
            url: pr.html_url.clone(),
        },
        threads: normalize_threads(pages.into_iter().flatten().collect()),
    })
}

#[derive(Debug, Clone, Deserialize)]
pub struct GitHubUser {
    pub login: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GitHubReviewComment {
    pub id: i64,
    pub body: Option<String>,
    #[serde(default)]
    pub path: String,
    pub line: Option<u64>,
    pub original_line: Option<u64>,
    pub in_reply_to_id: Option<i64>,
    pub created_at: Option<String>,
    pub user: Option<GitHubUser>,
}

fn root_id(comment: &GitHubReviewComment, by_id: &HashMap<i64, &GitHubReviewComment>) -> i64 {
    let mut current = comment;
    let mut hops = 0;
    while let Some(parent) = current.in_reply_to_id.and_then(|id| by_id.get(&id)) {
        current = parent;
        hops += 1;
        if hops > by_id.len() {
            break; // guard against reply cycles
        }
    }
    current.id
}

/// Groups replies under their root comment; threads ordered by last activity (oldest first).
pub fn normalize_threads(comments: Vec<GitHubReviewComment>) -> Vec<NormalizedThread> {
    let by_id: HashMap<i64, &GitHubReviewComment> = comments.iter().map(|c| (c.id, c)).collect();

    let mut order: Vec<i64> = Vec::new();
    let mut grouped: HashMap<i64, Vec<&GitHubReviewComment>> = HashMap::new();
    for comment in &comments {
        let root = root_id(comment, &by_id);
        grouped
            .entry(root)
            .or_insert_with(|| {
                order.push(root);
                Vec::new()
            })
            .push(comment);
    }

    let mut threads: Vec<NormalizedThread> = order
        .into_iter()
        .map(|root_id| {
            let mut sorted = grouped.remove(&root_id).unwrap_or_default();
            sorted.sort_by_key(|c| timestamp(c.created_at.as_deref()));
            let root = by_id.get(&root_id).copied().unwrap_or(sorted[0]);
            let line = root.line.or(root.original_line);
            let location = match line.filter(|&l| l > 0) {
                Some(line) => format!("{}:{line}", root.path),
                None => root.path.clone(),
            };
            NormalizedThread {
                id: root_id,
                status: "active".into(),
                file_path: Some(root.path.clone()),
                line,
                location,
                updated_at: sorted.last().and_then(|c| c.created_at.clone()),
                comments: sorted
                    .iter()
                    .map(|c| NormalizedComment {
                        id: c.id,
                        parent_comment_id: c.in_reply_to_id,
                        author: c
                            .user
                            .as_ref()
                            .and_then(|u| u.login.clone())
                            .filter(|l| !l.is_empty())
                            .unwrap_or_else(|| "Unknown".into()),
                        body: c.body.clone().unwrap_or_default().trim().to_string(),
                        created_at: c.created_at.clone(),
                    })
                    .collect(),
            }
        })
        .collect();

    threads.sort_by_key(|t| timestamp(t.updated_at.as_deref()));
    threads
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_remotes() {
        let expected = GitHubRepo {
            owner: "eimerreis".into(),
            name: "eimer-cli".into(),
        };
        assert_eq!(
            parse_repo_from_remote("git@github.com:eimerreis/eimer-cli.git"),
            Some(expected.clone())
        );
        assert_eq!(
            parse_repo_from_remote("https://github.com/eimerreis/eimer-cli"),
            Some(expected.clone())
        );
        assert_eq!(
            parse_repo_from_remote("ssh://git@GitHub.com/eimerreis/eimer-cli.git"),
            Some(expected)
        );
        assert_eq!(parse_repo_from_remote("https://github.com/eimerreis"), None);
        assert_eq!(
            parse_repo_from_remote("https://dev.azure.com/o/p/_git/r"),
            None
        );
        assert_eq!(parse_repo_override("a/b").unwrap().slug(), "a/b");
        assert!(parse_repo_override("ab").is_err());
    }

    #[test]
    fn groups_reply_chains() {
        let comments: Vec<GitHubReviewComment> = serde_json::from_str(
            r#"[
              {"id": 10, "body": "root A", "path": "a.ts", "line": 3, "created_at": "2024-01-01T10:00:00Z", "user": {"login": "ann"}},
              {"id": 20, "body": "root B", "path": "b.ts", "line": null, "original_line": 7, "created_at": "2024-01-01T09:00:00Z", "user": {"login": "bob"}},
              {"id": 12, "body": "reply 2", "path": "a.ts", "in_reply_to_id": 11, "created_at": "2024-01-01T12:00:00Z"},
              {"id": 11, "body": "reply 1", "path": "a.ts", "in_reply_to_id": 10, "created_at": "2024-01-01T11:00:00Z", "user": {"login": "bob"}}
            ]"#,
        )
        .unwrap();
        let threads = normalize_threads(comments);
        assert_eq!(threads.len(), 2);
        // B's last activity (09:00) is older than A's (12:00).
        assert_eq!(threads[0].id, 20);
        assert_eq!(threads[0].location, "b.ts:7");
        let a = &threads[1];
        assert_eq!(a.id, 10);
        assert_eq!(a.location, "a.ts:3");
        let ids: Vec<i64> = a.comments.iter().map(|c| c.id).collect();
        assert_eq!(ids, vec![10, 11, 12]);
        assert_eq!(a.comments[2].author, "Unknown");
        assert_eq!(a.comments[2].parent_comment_id, Some(11));
        assert_eq!(a.updated_at.as_deref(), Some("2024-01-01T12:00:00Z"));
    }
}

#[cfg(test)]
mod paginate_tests {
    use super::parse_paginated;

    #[test]
    fn parses_concatenated_pages() {
        let pages = parse_paginated::<u32>("[1,2]\n[3]").unwrap();
        assert_eq!(pages, vec![vec![1, 2], vec![3]]);
        assert!(parse_paginated::<u32>("").unwrap().is_empty());
        assert!(parse_paginated::<u32>("[1,").is_err());
    }
}
