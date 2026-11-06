//! `pr show` model (JSON shape), status mapping, merge-readiness rules and rendering.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use super::azdo::AzurePullRequest;
use super::shared::Platform;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CheckStatus {
    Pass,
    Fail,
    Pending,
    Unknown,
}

impl CheckStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            CheckStatus::Pass => "pass",
            CheckStatus::Fail => "fail",
            CheckStatus::Pending => "pending",
            CheckStatus::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub struct ShowReason {
    pub summary: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

impl ShowReason {
    fn new(summary: impl Into<String>) -> Self {
        Self {
            summary: summary.into(),
            details: None,
            url: None,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShowPr {
    pub id: u64,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    pub repository: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_branch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    pub is_draft: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShowComments {
    pub has_comments: bool,
    pub thread_count: usize,
    pub comment_count: usize,
}

#[derive(Debug, Serialize)]
pub struct Approval {
    pub approved: bool,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CheckDetail {
    pub name: String,
    pub status: CheckStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct Checks {
    pub status: CheckStatus,
    pub passed: usize,
    pub failed: usize,
    pub pending: usize,
    pub details: Vec<CheckDetail>,
}

impl Checks {
    pub fn from_details(details: Vec<CheckDetail>) -> Self {
        let count = |s: CheckStatus| details.iter().filter(|d| d.status == s).count();
        let (passed, failed, pending) = (
            count(CheckStatus::Pass),
            count(CheckStatus::Fail),
            count(CheckStatus::Pending),
        );
        Self {
            status: summarize_check_status(passed, failed, pending, details.len()),
            passed,
            failed,
            pending,
            details,
        }
    }

    fn names_with(&self, status: CheckStatus) -> impl Iterator<Item = &CheckDetail> {
        self.details.iter().filter(move |d| d.status == status)
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShowResult {
    pub platform: Platform,
    pub pr: ShowPr,
    pub comments: ShowComments,
    pub approval: Approval,
    pub checks: Checks,
    pub ready_to_merge: bool,
    pub not_ready_reasons: Vec<ShowReason>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitHubPrDetail {
    pub number: u64,
    #[serde(default)]
    pub title: String,
    pub url: Option<String>,
    pub state: Option<String>,
    pub is_draft: Option<bool>,
    pub review_decision: Option<String>,
    pub mergeable: Option<String>,
    pub merge_state_status: Option<String>,
    pub head_ref_name: Option<String>,
    pub base_ref_name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct GitHubCheck {
    pub name: Option<String>,
    pub bucket: Option<String>,
    pub state: Option<String>,
    pub link: Option<String>,
    pub workflow: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PolicyType {
    #[serde(rename = "displayName")]
    pub display_name: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PolicyConfiguration {
    pub is_blocking: Option<bool>,
    #[serde(rename = "type")]
    pub kind: Option<PolicyType>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PolicyContext {
    pub build_id: Option<u64>,
    pub build_definition_name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AzurePolicy {
    pub status: Option<String>,
    pub configuration: Option<PolicyConfiguration>,
    pub context: Option<PolicyContext>,
}

impl AzurePolicy {
    fn type_name(&self) -> Option<&str> {
        self.configuration
            .as_ref()
            .and_then(|c| c.kind.as_ref())
            .and_then(|t| t.display_name.as_deref())
            .filter(|n| !n.is_empty())
    }

    fn is_type(&self, name: &str) -> bool {
        self.type_name()
            .unwrap_or_default()
            .eq_ignore_ascii_case(name)
    }

    fn status_is(&self, status: &str) -> bool {
        self.status
            .as_deref()
            .unwrap_or_default()
            .eq_ignore_ascii_case(status)
    }
}

pub fn normalize_github_check_status(bucket: Option<&str>, state: Option<&str>) -> CheckStatus {
    match bucket.unwrap_or_default().to_lowercase().as_str() {
        // Skipped checks don't block merging (same as gh).
        "pass" | "skipping" => return CheckStatus::Pass,
        "fail" | "cancel" => return CheckStatus::Fail,
        "pending" => return CheckStatus::Pending,
        _ => {}
    }
    match state.unwrap_or_default().to_lowercase().as_str() {
        "success" | "succeeded" | "completed" => CheckStatus::Pass,
        "failure" | "failed" | "error" | "cancelled" | "canceled" => CheckStatus::Fail,
        "queued" | "pending" | "in_progress" | "in progress" | "requested" | "waiting" => {
            CheckStatus::Pending
        }
        _ => CheckStatus::Unknown,
    }
}

pub fn normalize_azure_policy_status(status: Option<&str>) -> CheckStatus {
    match status.unwrap_or_default().to_lowercase().as_str() {
        "approved" => CheckStatus::Pass,
        "rejected" => CheckStatus::Fail,
        "queued" | "running" | "pending" | "notset" => CheckStatus::Pending,
        _ => CheckStatus::Unknown,
    }
}

pub fn summarize_check_status(
    passed: usize,
    failed: usize,
    pending: usize,
    total: usize,
) -> CheckStatus {
    if failed > 0 {
        CheckStatus::Fail
    } else if pending > 0 {
        CheckStatus::Pending
    } else if passed > 0 || total == 0 {
        CheckStatus::Pass
    } else {
        CheckStatus::Unknown
    }
}

/// One entry of `gh pr view --json statusCheckRollup`: a check run or a commit status.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RollupItem {
    // CheckRun
    pub name: Option<String>,
    pub status: Option<String>,
    pub conclusion: Option<String>,
    pub details_url: Option<String>,
    pub workflow_name: Option<String>,
    // StatusContext
    pub context: Option<String>,
    pub state: Option<String>,
    pub target_url: Option<String>,
}

/// gh's check buckets: pass, fail, pending, skipping, cancel.
pub fn rollup_bucket(item: &RollupItem) -> &'static str {
    if let Some(state) = item.state.as_deref() {
        return match state.to_ascii_uppercase().as_str() {
            "SUCCESS" => "pass",
            "PENDING" | "EXPECTED" => "pending",
            _ => "fail",
        };
    }
    if !item
        .status
        .as_deref()
        .is_some_and(|s| s.eq_ignore_ascii_case("COMPLETED"))
    {
        return "pending";
    }
    match item
        .conclusion
        .as_deref()
        .unwrap_or_default()
        .to_ascii_uppercase()
        .as_str()
    {
        "SUCCESS" | "NEUTRAL" => "pass",
        "SKIPPED" => "skipping",
        "CANCELLED" => "cancel",
        _ => "fail",
    }
}

impl From<RollupItem> for GitHubCheck {
    fn from(item: RollupItem) -> Self {
        let bucket = rollup_bucket(&item).to_string();
        GitHubCheck {
            name: item.name.or(item.context),
            bucket: Some(bucket),
            state: item.conclusion.or(item.state),
            link: item.details_url.or(item.target_url),
            workflow: item.workflow_name,
        }
    }
}

pub fn github_checks(checks: Vec<GitHubCheck>) -> Checks {
    let first = |values: [Option<String>; 2]| values.into_iter().flatten().find(|v| !v.is_empty());
    Checks::from_details(
        checks
            .into_iter()
            .map(|c| CheckDetail {
                status: normalize_github_check_status(c.bucket.as_deref(), c.state.as_deref()),
                name: first([c.name, c.workflow]).unwrap_or_else(|| "Unnamed check".into()),
                url: c.link,
            })
            .collect(),
    )
}

/// Build policies become checks, linked to the build results page when possible.
/// `project_base_url` is e.g. `https://dev.azure.com/org/project` (may be empty).
pub fn azure_checks(policies: &[AzurePolicy], project_base_url: &str) -> Checks {
    Checks::from_details(
        policies
            .iter()
            .filter(|p| p.is_type("build"))
            .map(|p| {
                let ctx = p.context.as_ref();
                let name = ctx
                    .and_then(|c| c.build_definition_name.clone())
                    .filter(|n| !n.is_empty())
                    .unwrap_or_else(|| "Build".into());
                let build_id = ctx.and_then(|c| c.build_id).filter(|&id| id > 0);
                CheckDetail {
                    name: match build_id {
                        Some(id) => format!("{name} (#{id})"),
                        None => name,
                    },
                    status: normalize_azure_policy_status(p.status.as_deref()),
                    url: build_id
                        .filter(|_| !project_base_url.is_empty())
                        .map(|id| format!("{project_base_url}/_build/results?buildId={id}")),
                }
            })
            .collect(),
    )
}

pub fn azure_approval(pr: &AzurePullRequest, policies: &[AzurePolicy]) -> Approval {
    let approvers: Vec<String> = pr
        .reviewers
        .iter()
        .flatten()
        .filter(|r| r.vote.unwrap_or(0) >= 5)
        .map(|r| {
            r.display_name
                .clone()
                .filter(|n| !n.is_empty())
                .unwrap_or_else(|| "Unknown".into())
        })
        .filter(|n| !n.trim().is_empty())
        .collect();
    let policy_approved = policies
        .iter()
        .any(|p| p.is_type("minimum number of reviewers") && p.status_is("approved"));

    let approved = !approvers.is_empty() || policy_approved;
    let summary = if !approvers.is_empty() {
        format!("Approved by {}", approvers.join(", "))
    } else if policy_approved {
        "Reviewer policy approved".into()
    } else {
        "No approvals yet".into()
    };
    Approval { approved, summary }
}

fn check_reasons(
    reasons: &mut Vec<ShowReason>,
    checks: &Checks,
    failed_prefix: &str,
    pending_summary: &str,
) {
    match checks.status {
        CheckStatus::Fail => {
            for failed in checks.names_with(CheckStatus::Fail) {
                reasons.push(ShowReason {
                    summary: format!("{failed_prefix}: {}", failed.name),
                    details: None,
                    url: failed.url.clone(),
                });
            }
        }
        CheckStatus::Pending => {
            let names: Vec<&str> = checks
                .names_with(CheckStatus::Pending)
                .map(|d| d.name.as_str())
                .take(3)
                .collect();
            reasons.push(ShowReason {
                summary: pending_summary.into(),
                details: (!names.is_empty()).then(|| names.join(", ")),
                url: None,
            });
        }
        _ => {}
    }
}

pub fn github_not_ready_reasons(
    detail: &GitHubPrDetail,
    approved: bool,
    checks: &Checks,
) -> Vec<ShowReason> {
    let mut reasons = Vec::new();
    let state = detail.state.as_deref().filter(|s| !s.is_empty());
    if !state.unwrap_or_default().eq_ignore_ascii_case("open") {
        reasons.push(ShowReason::new(format!(
            "PR state is '{}'",
            state.unwrap_or("unknown")
        )));
    }
    if detail.is_draft.unwrap_or(false) {
        reasons.push(ShowReason::new("PR is marked as draft"));
    }
    if detail
        .mergeable
        .as_deref()
        .unwrap_or_default()
        .eq_ignore_ascii_case("conflicting")
    {
        reasons.push(ShowReason::new("PR has merge conflicts"));
    }
    if !approved {
        reasons.push(ShowReason::new("PR is not approved"));
    }
    check_reasons(
        &mut reasons,
        checks,
        "Check failed",
        "Checks are still pending",
    );

    let merge_state = detail
        .merge_state_status
        .as_deref()
        .unwrap_or_default()
        .to_uppercase();
    if merge_state == "BLOCKED" || merge_state == "DIRTY" {
        reasons.push(ShowReason::new(format!(
            "Merge state is {}",
            merge_state.to_lowercase()
        )));
    }
    dedupe(reasons)
}

pub fn azure_not_ready_reasons(
    detail: &AzurePullRequest,
    approved: bool,
    checks: &Checks,
    policies: &[AzurePolicy],
) -> Vec<ShowReason> {
    let mut reasons = Vec::new();
    let status = detail.status.as_deref().filter(|s| !s.is_empty());
    if !status.unwrap_or_default().eq_ignore_ascii_case("active") {
        reasons.push(ShowReason::new(format!(
            "PR status is '{}'",
            status.unwrap_or("unknown")
        )));
    }
    if detail.is_draft.unwrap_or(false) {
        reasons.push(ShowReason::new("PR is marked as draft"));
    }
    if detail
        .merge_status
        .as_deref()
        .unwrap_or_default()
        .eq_ignore_ascii_case("conflicts")
    {
        reasons.push(ShowReason::new("PR has merge conflicts"));
    }
    if !approved {
        reasons.push(ShowReason::new("PR is not approved"));
    }
    check_reasons(
        &mut reasons,
        checks,
        "Build failed",
        "Build checks are still pending",
    );

    for policy in policies {
        let blocking = policy
            .configuration
            .as_ref()
            .and_then(|c| c.is_blocking)
            .unwrap_or(false);
        if !blocking || policy.is_type("build") {
            continue;
        }
        if policy.status_is("approved") || policy.status_is("notapplicable") {
            continue;
        }
        reasons.push(ShowReason {
            summary: format!(
                "Blocking policy not satisfied: {}",
                policy.type_name().unwrap_or("Policy")
            ),
            details: Some(format!(
                "status={}",
                policy
                    .status
                    .as_deref()
                    .filter(|s| !s.is_empty())
                    .unwrap_or("unknown")
            )),
            url: None,
        });
    }
    dedupe(reasons)
}

fn dedupe(reasons: Vec<ShowReason>) -> Vec<ShowReason> {
    let mut seen = HashSet::new();
    reasons
        .into_iter()
        .filter(|r| seen.insert(r.clone()))
        .collect()
}

#[cfg(test)]
#[path = "show_helpers_tests.rs"]
mod tests;
