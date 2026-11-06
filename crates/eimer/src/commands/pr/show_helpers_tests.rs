use super::*;

fn policy(json: &str) -> AzurePolicy {
    serde_json::from_str(json).unwrap()
}

#[test]
fn maps_check_statuses() {
    use CheckStatus::*;
    assert_eq!(normalize_github_check_status(Some("pass"), None), Pass);
    assert_eq!(normalize_github_check_status(Some("cancel"), None), Fail);
    assert_eq!(
        normalize_github_check_status(Some("skipping"), Some("SUCCESS")),
        Pass
    );
    assert_eq!(
        normalize_github_check_status(None, Some("IN_PROGRESS")),
        Pending
    );
    assert_eq!(normalize_github_check_status(None, Some("weird")), Unknown);
    assert_eq!(normalize_azure_policy_status(Some("approved")), Pass);
    assert_eq!(normalize_azure_policy_status(Some("rejected")), Fail);
    assert_eq!(normalize_azure_policy_status(Some("notSet")), Pending);
    assert_eq!(
        normalize_azure_policy_status(Some("notApplicable")),
        Unknown
    );
    assert_eq!(summarize_check_status(0, 0, 0, 0), Pass);
    assert_eq!(summarize_check_status(1, 1, 1, 3), Fail);
    assert_eq!(summarize_check_status(1, 0, 1, 2), Pending);
    assert_eq!(summarize_check_status(0, 0, 0, 2), Unknown);
}

#[test]
fn azure_checks_approval_and_reasons() {
    let policies = vec![
        policy(
            r#"{"status": "rejected", "configuration": {"isBlocking": true, "type": {"displayName": "Build"}}, "context": {"buildId": 9, "buildDefinitionName": "CI"}}"#,
        ),
        policy(
            r#"{"status": "approved", "configuration": {"isBlocking": true, "type": {"displayName": "Minimum number of reviewers"}}}"#,
        ),
        policy(
            r#"{"status": "queued", "configuration": {"isBlocking": true, "type": {"displayName": "Work item linking"}}}"#,
        ),
        policy(
            r#"{"status": "queued", "configuration": {"isBlocking": false, "type": {"displayName": "Comment requirements"}}}"#,
        ),
    ];
    let checks = azure_checks(&policies, "https://dev.azure.com/o/p");
    assert_eq!(checks.status, CheckStatus::Fail);
    assert_eq!(checks.details[0].name, "CI (#9)");
    assert_eq!(
        checks.details[0].url.as_deref(),
        Some("https://dev.azure.com/o/p/_build/results?buildId=9")
    );

    let pr: AzurePullRequest = serde_json::from_str(
        r#"{"pullRequestId": 1, "title": "t", "status": "active", "mergeStatus": "conflicts",
            "reviewers": [{"displayName": "Ann", "vote": 10}, {"displayName": "Bob", "vote": 0}]}"#,
    )
    .unwrap();
    let approval = azure_approval(&pr, &policies);
    assert!(approval.approved);
    assert_eq!(approval.summary, "Approved by Ann");

    let reasons = azure_not_ready_reasons(&pr, approval.approved, &checks, &policies);
    let summaries: Vec<&str> = reasons.iter().map(|r| r.summary.as_str()).collect();
    assert_eq!(
        summaries,
        vec![
            "PR has merge conflicts",
            "Build failed: CI (#9)",
            "Blocking policy not satisfied: Work item linking"
        ]
    );
    assert_eq!(reasons[2].details.as_deref(), Some("status=queued"));

    let no_reviewers: AzurePullRequest =
        serde_json::from_str(r#"{"pullRequestId": 1, "title": "t"}"#).unwrap();
    assert_eq!(
        azure_approval(&no_reviewers, &policies).summary,
        "Reviewer policy approved"
    );
    assert_eq!(
        azure_approval(&no_reviewers, &[]).summary,
        "No approvals yet"
    );
}

#[test]
fn github_reasons_and_json_shape() {
    let detail: GitHubPrDetail = serde_json::from_str(
        r#"{"number": 3, "title": "t", "state": "OPEN", "isDraft": true, "reviewDecision": "",
            "mergeable": "CONFLICTING", "mergeStateStatus": "DIRTY"}"#,
    )
    .unwrap();
    let checks = github_checks(
        serde_json::from_str(
            r#"[{"name": "lint", "bucket": "pending"}, {"workflow": "ci", "bucket": "pending"},
                {"name": "", "bucket": "pass"}]"#,
        )
        .unwrap(),
    );
    assert_eq!(checks.status, CheckStatus::Pending);
    assert_eq!(checks.details[2].name, "Unnamed check");
    let reasons = github_not_ready_reasons(&detail, false, &checks);
    let summaries: Vec<&str> = reasons.iter().map(|r| r.summary.as_str()).collect();
    assert_eq!(
        summaries,
        vec![
            "PR is marked as draft",
            "PR has merge conflicts",
            "PR is not approved",
            "Checks are still pending",
            "Merge state is dirty"
        ]
    );
    assert_eq!(reasons[3].details.as_deref(), Some("lint, ci"));

    let json = serde_json::to_value(&reasons[3]).unwrap();
    assert_eq!(
        json,
        serde_json::json!({"summary": "Checks are still pending", "details": "lint, ci"})
    );
    let json = serde_json::to_value(&checks).unwrap();
    assert_eq!(
        json["details"][0],
        serde_json::json!({"name": "lint", "status": "pending"})
    );
}

#[test]
fn dedupes_reasons() {
    let r = ShowReason::new("x");
    assert_eq!(dedupe(vec![r.clone(), r.clone()]), vec![r]);
}
