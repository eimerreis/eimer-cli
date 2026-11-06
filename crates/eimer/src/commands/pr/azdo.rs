//! Azure DevOps PR lookups (`az repos pr ...`) and comment thread loading (REST via az token).

use anyhow::{Context, Result, anyhow, bail};
use serde::Deserialize;

use super::shared::{Platform, Warnings, argv, timestamp};
use super::threads::{
    NormalizedComment, NormalizedResult, NormalizedThread, ResultPr, build_location,
    map_azure_status,
};
use crate::core::azure::AzScope;
use crate::core::process::run_json;

const AZURE_DEVOPS_RESOURCE: &str = "499b84ac-1321-427f-aa17-267ca6975798";

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AzureRepository {
    pub name: Option<String>,
    pub web_url: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AzureIdentity {
    pub display_name: Option<String>,
    pub unique_name: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AzureReviewer {
    pub display_name: Option<String>,
    pub unique_name: Option<String>,
    pub vote: Option<i64>,
}

/// Union of the fields `az repos pr show/list` return that the PR commands use.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AzurePullRequest {
    pub pull_request_id: u64,
    #[serde(default)]
    pub title: String,
    pub source_ref_name: Option<String>,
    pub target_ref_name: Option<String>,
    pub url: Option<String>,
    pub status: Option<String>,
    pub is_draft: Option<bool>,
    pub merge_status: Option<String>,
    pub creation_date: Option<String>,
    pub closed_date: Option<String>,
    pub repository: Option<AzureRepository>,
    pub created_by: Option<AzureIdentity>,
    pub reviewers: Option<Vec<AzureReviewer>>,
}

impl AzurePullRequest {
    pub fn repo_name(&self) -> Option<&str> {
        self.repository
            .as_ref()
            .and_then(|r| r.name.as_deref())
            .filter(|n| !n.is_empty())
    }

    pub fn repo_web_url(&self) -> Option<&str> {
        self.repository
            .as_ref()
            .and_then(|r| r.web_url.as_deref())
            .filter(|u| !u.is_empty())
    }
}

/// `az <parts> --detect true --output <output>`.
pub fn az(parts: &[&str], output: &str) -> Vec<String> {
    let mut cmd = argv(parts);
    cmd.extend(AzScope::detect().args());
    cmd.extend(["--output".into(), output.into()]);
    cmd
}

pub fn show_pr(id: u64) -> Result<AzurePullRequest> {
    let id = id.to_string();
    run_json(az(&["az", "repos", "pr", "show", "--id", &id], "json"))
}

/// `None` when the PR cannot be loaded (mirrors the TS lookup that swallowed errors).
pub fn find_pr_by_id(id: u64) -> Option<AzurePullRequest> {
    show_pr(id).ok()
}

/// Most recent active PR for `branch` (tries the short name, then `refs/heads/<branch>`).
pub fn find_pr_by_branch(
    branch: &str,
    repo: Option<&str>,
    warnings: &mut Warnings,
) -> Result<Option<AzurePullRequest>> {
    let mut base = az(
        &[
            "az", "repos", "pr", "list", "--status", "active", "--top", "20",
        ],
        "json",
    );
    if let Some(repo) = repo {
        base.extend(["--repository".into(), repo.into()]);
    }
    let list = |source: String| -> Result<Vec<AzurePullRequest>> {
        let mut cmd = base.clone();
        cmd.extend(["--source-branch".into(), source]);
        run_json(&cmd)
    };

    let mut prs = list(branch.to_string())?;
    if prs.is_empty() {
        prs = list(format!("refs/heads/{branch}"))?;
    }
    if prs.is_empty() {
        return Ok(None);
    }

    prs.sort_by_key(|pr| std::cmp::Reverse(timestamp(pr.creation_date.as_deref())));
    if prs.len() > 1 {
        warnings.push(format!(
            "Found {} Azure PRs for branch '{branch}'. Using most recent (#{}).",
            prs.len(),
            prs[0].pull_request_id
        ));
    }
    Ok(prs.into_iter().next())
}

/// Finds a PR by id or branch; `None` when nothing matches.
pub fn find_pr(
    id: Option<u64>,
    branch: &str,
    repo: Option<&str>,
    warnings: &mut Warnings,
) -> Result<Option<AzurePullRequest>> {
    match id {
        Some(id) => Ok(find_pr_by_id(id)),
        None => find_pr_by_branch(branch, repo, warnings),
    }
}

/// `pr comments` for Azure DevOps: by id (errors propagate) or branch (error when none found).
pub fn load_comments(
    branch: &str,
    id: Option<u64>,
    repo: Option<&str>,
    warnings: &mut Warnings,
) -> Result<NormalizedResult> {
    let pr = match id {
        Some(id) => show_pr(id)?,
        None => find_pr_by_branch(branch, repo, warnings)?.ok_or_else(|| {
            anyhow!(
                "No active Azure DevOps PR found for branch '{branch}'. Use --id to target a PR directly."
            )
        })?,
    };
    load_comments_for(&pr, branch, repo)
}

/// Loads and normalizes the comment threads of an already-fetched PR.
pub fn load_comments_for(
    pr: &AzurePullRequest,
    branch: &str,
    repo: Option<&str>,
) -> Result<NormalizedResult> {
    let url = pr.url.as_deref().filter(|u| !u.is_empty()).ok_or_else(|| {
        anyhow!("Azure PR payload did not include a URL. Cannot fetch comment threads.")
    })?;
    let token = access_token()?;
    let threads = fetch_threads(url, &token)?;

    Ok(NormalizedResult {
        platform: Platform::AzureDevops,
        pr: ResultPr {
            id: pr.pull_request_id,
            title: pr.title.clone(),
            branch: Some(
                pr.source_ref_name
                    .clone()
                    .filter(|b| !b.is_empty())
                    .unwrap_or_else(|| branch.to_string()),
            ),
            repository: pr
                .repo_name()
                .map(String::from)
                .or_else(|| repo.map(String::from)),
            url: None,
        },
        threads: normalize_threads(threads),
    })
}

fn access_token() -> Result<String> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Token {
        access_token: Option<String>,
    }
    let token: Token = run_json(argv(&[
        "az",
        "account",
        "get-access-token",
        "--resource",
        AZURE_DEVOPS_RESOURCE,
        "--output",
        "json",
    ]))?;
    token
        .access_token
        .filter(|t| !t.is_empty())
        .ok_or_else(|| anyhow!("Failed to acquire Azure DevOps access token from Azure CLI."))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AzureComment {
    pub id: i64,
    pub parent_comment_id: Option<i64>,
    pub content: Option<String>,
    pub comment_type: Option<String>,
    pub is_deleted: Option<bool>,
    pub published_date: Option<String>,
    pub author: Option<AzureIdentity>,
}

#[derive(Debug, Deserialize)]
pub struct FilePosition {
    pub line: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadContext {
    pub file_path: Option<String>,
    pub right_file_start: Option<FilePosition>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AzureThread {
    pub id: i64,
    pub status: Option<String>,
    pub is_deleted: Option<bool>,
    pub last_updated_date: Option<String>,
    pub thread_context: Option<ThreadContext>,
    pub comments: Option<Vec<AzureComment>>,
}

fn fetch_threads(pr_url: &str, token: &str) -> Result<Vec<AzureThread>> {
    #[derive(Deserialize)]
    struct Payload {
        value: Option<Vec<AzureThread>>,
    }
    let client = crate::core::http::client();
    let response = client
        .get(format!("{pr_url}/threads?api-version=7.1"))
        .bearer_auth(token)
        .header("Accept", "application/json")
        .send()
        .context("Failed to fetch Azure PR threads")?;
    let status = response.status();
    let body = response.text()?;
    if !status.is_success() {
        bail!(
            "Failed to fetch Azure PR threads ({}): {body}",
            status.as_u16()
        );
    }
    let payload: Payload =
        serde_json::from_str(&body).context("Failed to parse Azure PR threads response")?;
    Ok(payload.value.unwrap_or_default())
}

fn is_text_comment(comment: &AzureComment) -> bool {
    !comment.is_deleted.unwrap_or(false)
        && comment
            .comment_type
            .as_deref()
            .is_some_and(|t| t.eq_ignore_ascii_case("text"))
}

/// Keeps non-deleted threads with at least one live text comment (drops system threads).
pub fn normalize_threads(threads: Vec<AzureThread>) -> Vec<NormalizedThread> {
    threads
        .into_iter()
        .filter(|t| !t.is_deleted.unwrap_or(false))
        .filter(|t| t.comments.iter().flatten().any(is_text_comment))
        .map(|thread| {
            let file_path = thread
                .thread_context
                .as_ref()
                .and_then(|c| c.file_path.clone());
            let line = thread
                .thread_context
                .as_ref()
                .and_then(|c| c.right_file_start.as_ref())
                .and_then(|p| p.line);
            let comments = thread
                .comments
                .unwrap_or_default()
                .into_iter()
                .filter(is_text_comment)
                .map(|c| NormalizedComment {
                    id: c.id,
                    parent_comment_id: c.parent_comment_id,
                    author: c
                        .author
                        .and_then(|a| a.display_name)
                        .filter(|n| !n.is_empty())
                        .unwrap_or_else(|| "Unknown".into()),
                    body: c.content.unwrap_or_default().trim().to_string(),
                    created_at: c.published_date,
                })
                .collect();
            NormalizedThread {
                id: thread.id,
                status: map_azure_status(thread.status.as_deref()),
                location: build_location(file_path.as_deref(), line),
                file_path,
                line,
                comments,
                updated_at: thread.last_updated_date,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_threads() {
        let threads: Vec<AzureThread> = serde_json::from_str(
            r#"[
              {"id": 1, "status": "active", "comments": [{"id": 1, "parentCommentId": 0, "commentType": "system", "content": "voted"}]},
              {"id": 2, "status": "wontFix", "isDeleted": true, "comments": [{"id": 1, "commentType": "text", "content": "x"}]},
              {"id": 3, "status": "fixed", "lastUpdatedDate": "2024-01-01T00:00:00Z",
               "threadContext": {"filePath": "/src/a.ts", "rightFileStart": {"line": 4}},
               "comments": [
                 {"id": 1, "parentCommentId": 0, "commentType": "text", "content": " Fix this \n", "author": {"displayName": "Ann"}, "publishedDate": "2024-01-01T00:00:00Z"},
                 {"id": 2, "parentCommentId": 1, "commentType": "text", "content": "deleted", "isDeleted": true},
                 {"id": 3, "parentCommentId": 1, "commentType": "text", "content": "done"}
               ]},
              {"id": 4, "status": "byDesign", "comments": [{"id": 1, "parentCommentId": 0, "commentType": "text", "content": "general"}]}
            ]"#,
        )
        .unwrap();
        let normalized = normalize_threads(threads);
        assert_eq!(normalized.len(), 2);
        let t = &normalized[0];
        assert_eq!(
            (t.id, t.status.as_str(), t.location.as_str()),
            (3, "fixed", "/src/a.ts:4")
        );
        assert_eq!(t.comments.len(), 2);
        assert_eq!(t.comments[0].body, "Fix this");
        assert_eq!(t.comments[0].author, "Ann");
        assert_eq!(t.comments[1].author, "Unknown");
        assert_eq!(t.comments[1].parent_comment_id, Some(1));
        assert_eq!(normalized[1].location, "(general)");
        assert_eq!(normalized[1].status, "by design");
    }

    #[test]
    fn builds_az_commands() {
        assert_eq!(
            az(&["az", "repos", "pr", "show", "--id", "5"], "json"),
            vec![
                "az", "repos", "pr", "show", "--id", "5", "--detect", "true", "--output", "json"
            ]
        );
    }
}
