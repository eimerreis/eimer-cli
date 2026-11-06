//! Platform-neutral review comment threads (`pr comments` JSON shape) and their rendering.

use serde::Serialize;

use super::shared::Platform;
use crate::core::ui::{self, symbols};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NormalizedComment {
    pub id: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_comment_id: Option<i64>,
    pub author: String,
    pub body: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NormalizedThread {
    pub id: i64,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<u64>,
    pub location: String,
    pub comments: Vec<NormalizedComment>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultPr {
    pub id: u64,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repository: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NormalizedResult {
    pub platform: Platform,
    pub pr: ResultPr,
    pub threads: Vec<NormalizedThread>,
}

impl NormalizedResult {
    pub fn comment_count(&self) -> usize {
        self.threads.iter().map(|t| t.comments.len()).sum()
    }
}

/// Azure thread status as shown to users.
pub fn map_azure_status(status: Option<&str>) -> String {
    let status = status.unwrap_or("unknown").to_lowercase();
    match status.as_str() {
        "wontfix" => "won't fix".into(),
        "bydesign" => "by design".into(),
        _ => status,
    }
}

/// "path:line", "path" or "(general)".
pub fn build_location(file_path: Option<&str>, line: Option<u64>) -> String {
    match (file_path.filter(|p| !p.is_empty()), line.filter(|&l| l > 0)) {
        (None, _) => "(general)".into(),
        (Some(path), None) => path.into(),
        (Some(path), Some(line)) => format!("{path}:{line}"),
    }
}

/// Non-empty lines of a comment body (CRLF normalized, trimmed).
pub fn body_lines(body: &str) -> Vec<String> {
    body.replace("\r\n", "\n")
        .trim()
        .split('\n')
        .filter(|l| !l.is_empty())
        .map(String::from)
        .collect()
}

pub fn print_human_readable(result: &NormalizedResult) {
    let platform_label = match result.platform {
        Platform::Github => "GitHub",
        Platform::AzureDevops => "Azure DevOps",
    };
    println!(
        "{} {}",
        ui::bold(&format!("PR #{}", result.pr.id)),
        result.pr.title
    );
    let repo_suffix = result
        .pr
        .repository
        .as_deref()
        .filter(|r| !r.is_empty())
        .map(|r| format!(" | {r}"))
        .unwrap_or_default();
    println!("{}", ui::dim(&format!("{platform_label}{repo_suffix}")));

    if result.threads.is_empty() {
        ui::print_info(
            "No review comment threads found.",
            Some("Try `pr show` to inspect approval and merge-readiness details."),
        );
        return;
    }

    let url = result.pr.url.as_deref().unwrap_or_default();
    for thread in &result.threads {
        println!();
        let updated = match thread.updated_at.as_deref() {
            Some(at) => ui::relative_time(Some(at)),
            None => "-".into(),
        };
        println!(
            "{}",
            ui::render_table(
                ["Location", "Status", "Updated"],
                [vec![
                    thread.location.clone(),
                    thread.status.clone(),
                    updated
                ]]
            )
        );

        for comment in &thread.comments {
            let authored_at = comment
                .created_at
                .as_deref()
                .map(|at| ui::dim(&format!(" ({})", ui::relative_time(Some(at)))))
                .unwrap_or_default();
            println!(
                "  {} {}{authored_at}",
                symbols::arrow(),
                ui::bold(&comment.author)
            );
            let lines = body_lines(&comment.body);
            if let Some((first, rest)) = lines.split_first() {
                println!("    {}", ui::terminal_link(first, url));
                for line in rest {
                    println!("    {line}");
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_statuses() {
        assert_eq!(map_azure_status(Some("wontFix")), "won't fix");
        assert_eq!(map_azure_status(Some("byDesign")), "by design");
        assert_eq!(map_azure_status(Some("Active")), "active");
        assert_eq!(map_azure_status(None), "unknown");
    }

    #[test]
    fn builds_locations() {
        assert_eq!(build_location(None, Some(3)), "(general)");
        assert_eq!(build_location(Some("src/a.rs"), None), "src/a.rs");
        assert_eq!(build_location(Some("src/a.rs"), Some(0)), "src/a.rs");
        assert_eq!(build_location(Some("src/a.rs"), Some(12)), "src/a.rs:12");
    }

    #[test]
    fn splits_body_lines() {
        assert_eq!(body_lines("  a\r\n\r\nb \n"), vec!["a", "b"]);
        assert!(body_lines("   ").is_empty());
    }

    #[test]
    fn serializes_without_missing_fields() {
        let result = NormalizedResult {
            platform: Platform::AzureDevops,
            pr: ResultPr {
                id: 1,
                title: "T".into(),
                branch: Some("b".into()),
                repository: None,
                url: None,
            },
            threads: vec![NormalizedThread {
                id: 2,
                status: "active".into(),
                file_path: None,
                line: None,
                location: "(general)".into(),
                comments: vec![NormalizedComment {
                    id: 3,
                    parent_comment_id: Some(0),
                    author: "A".into(),
                    body: "x".into(),
                    created_at: None,
                }],
                updated_at: None,
            }],
        };
        let json = serde_json::to_value(&result).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "platform": "azure-devops",
                "pr": {"id": 1, "title": "T", "branch": "b"},
                "threads": [{"id": 2, "status": "active", "location": "(general)",
                    "comments": [{"id": 3, "parentCommentId": 0, "author": "A", "body": "x"}]}]
            })
        );
    }
}
