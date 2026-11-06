//! `pr show [id]`: merge readiness (comments, approvals, checks, blocking policies).

use anyhow::Result;
use clap::Args;

use super::azdo::{self, az};
use super::github::{self, GitHubRepo};
use super::shared::{
    Platform, Target, TargetArgs, Warnings, argv, finish, run_json_allow_exit_codes,
    strip_ref_prefix,
};
use super::show_helpers::{
    Approval, AzurePolicy, CheckDetail, CheckStatus, Checks, GitHubCheck, GitHubPrDetail,
    ShowComments, ShowPr, ShowResult, azure_approval, azure_checks, azure_not_ready_reasons,
    github_checks, github_not_ready_reasons,
};
use super::threads::NormalizedResult;
use crate::core::azure::parse_remote;
use crate::core::process::run_json;
use crate::core::ui::{self, symbols};

#[derive(Args)]
pub struct ShowArgs {
    #[command(flatten)]
    target: TargetArgs,
    /// Print machine-readable JSON
    #[arg(long, short = 'j')]
    json: bool,
}

pub fn run(args: ShowArgs) -> Result<()> {
    let result = (|| -> Result<()> {
        let target = args.target.resolve()?;
        let mut warnings = Warnings::default();
        let result = ui::spin_unless(args.json, "Loading pull request details", || {
            match target.platform {
                Platform::Github => show_github(&target, &mut warnings),
                Platform::AzureDevops => show_azure(&target, &mut warnings),
            }
        });
        warnings.flush(args.json);

        let Some(result) = result? else {
            ui::print_info(
                &format!("No pull request found for {}.", target.lookup_label()),
                Some("Try `pr list --all` or pass --id / --repo to target a different PR."),
            );
            return Ok(());
        };

        if args.json {
            return ui::print_json(&result);
        }
        print_show_result(&result);
        Ok(())
    })();
    finish(
        result,
        "Failed to show PR",
        "Check your git remote and authenticate with `gh auth status` or `az login`.",
    )
}

fn comment_summary(comments: &NormalizedResult) -> ShowComments {
    let comment_count = comments.comment_count();
    ShowComments {
        has_comments: comment_count > 0,
        thread_count: comments.threads.len(),
        comment_count,
    }
}

fn show_github(target: &Target, warnings: &mut Warnings) -> Result<Option<ShowResult>> {
    let repo = github::parse_repo(&target.remote_url, target.repo.as_deref())?;
    let Some(pr) = github::find_pr(&repo, target.pr_id, &target.branch, warnings)? else {
        return Ok(None);
    };
    let slug = repo.slug();
    let detail: GitHubPrDetail = run_json(argv(&[
        "gh",
        "pr",
        "view",
        &pr.number.to_string(),
        "--repo",
        &slug,
        "--json",
        "number,title,url,state,isDraft,reviewDecision,mergeable,mergeStateStatus,headRefName,baseRefName",
    ]))?;

    let branch = detail
        .head_ref_name
        .clone()
        .filter(|b| !b.is_empty())
        .unwrap_or_else(|| target.branch.clone());
    let comments = github::load_comments_for(&repo, &pr, &branch)?;
    let checks = load_github_checks(&repo, detail.number)?;
    let review_decision = detail.review_decision.clone().filter(|d| !d.is_empty());
    let approved = review_decision
        .as_deref()
        .is_some_and(|d| d.eq_ignore_ascii_case("approved"));
    let reasons = github_not_ready_reasons(&detail, approved, &checks);

    Ok(Some(ShowResult {
        platform: Platform::Github,
        pr: ShowPr {
            id: detail.number,
            title: detail.title,
            url: detail.url,
            repository: slug,
            branch: Some(branch),
            target_branch: detail.base_ref_name,
            status: detail.state,
            is_draft: detail.is_draft.unwrap_or(false),
        },
        comments: comment_summary(&comments),
        approval: Approval {
            approved,
            summary: review_decision.unwrap_or_else(|| "REVIEW_REQUIRED".into()),
        },
        checks,
        ready_to_merge: reasons.is_empty(),
        not_ready_reasons: reasons,
    }))
}

/// `gh pr checks` exits 1 when checks fail and 8 when they are pending.
fn load_github_checks(repo: &GitHubRepo, number: u64) -> Result<Checks> {
    let checks: Option<Vec<GitHubCheck>> = run_json_allow_exit_codes(
        &argv(&[
            "gh",
            "pr",
            "checks",
            &number.to_string(),
            "--repo",
            &repo.slug(),
            "--json",
            "name,state,bucket,link,workflow",
        ]),
        &[0, 1, 8],
    )?;
    Ok(github_checks(checks.unwrap_or_default()))
}

fn show_azure(target: &Target, warnings: &mut Warnings) -> Result<Option<ShowResult>> {
    let repo = target.repo.as_deref();
    let Some(pr) = azdo::find_pr(target.pr_id, &target.branch, repo, warnings)? else {
        return Ok(None);
    };
    // A PR found by ID already is the `az repos pr show` payload; list results are re-fetched.
    let detail = if target.pr_id.is_some() {
        pr
    } else {
        azdo::show_pr(pr.pull_request_id)?
    };

    let branch = strip_ref_prefix(
        detail
            .source_ref_name
            .as_deref()
            .filter(|b| !b.is_empty())
            .or(Some(target.branch.as_str())),
    );
    let comments = azdo::load_comments_for(&detail, &branch, repo)?;

    let id = detail.pull_request_id.to_string();
    let policies: Vec<AzurePolicy> = run_json(az(
        &["az", "repos", "pr", "policy", "list", "--id", &id],
        "json",
    ))?;

    let base_url = detail
        .repo_web_url()
        .and_then(parse_remote)
        .or_else(|| parse_remote(&target.remote_url))
        .map(|p| p.base_url)
        .unwrap_or_default();
    let checks = azure_checks(&policies, &base_url);
    let approval = azure_approval(&detail, &policies);
    let reasons = azure_not_ready_reasons(&detail, approval.approved, &checks, &policies);

    Ok(Some(ShowResult {
        platform: Platform::AzureDevops,
        pr: ShowPr {
            id: detail.pull_request_id,
            url: Some(
                detail
                    .repo_web_url()
                    .map(|u| format!("{u}/pullrequest/{}", detail.pull_request_id))
                    .unwrap_or_default(),
            ),
            repository: detail
                .repo_name()
                .map(String::from)
                .or_else(|| target.repo.clone())
                .unwrap_or_else(|| "unknown".into()),
            branch: Some(branch),
            target_branch: Some(strip_ref_prefix(detail.target_ref_name.as_deref())),
            status: detail.status.clone(),
            is_draft: detail.is_draft.unwrap_or(false),
            title: detail.title,
        },
        comments: comment_summary(&comments),
        approval,
        checks,
        ready_to_merge: reasons.is_empty(),
        not_ready_reasons: reasons,
    }))
}

fn link_suffix(url: Option<&str>) -> String {
    url.filter(|u| !u.is_empty())
        .map(|u| format!(" | {}", ui::terminal_link(u, u)))
        .unwrap_or_default()
}

pub fn print_show_result(result: &ShowResult) {
    let pr = &result.pr;
    println!("{} {}", ui::bold(&format!("PR #{}", pr.id)), pr.title);
    println!(
        "{}",
        ui::dim(&format!("{} | {}", result.platform.id(), pr.repository))
    );
    let branch = pr.branch.as_deref().filter(|b| !b.is_empty());
    let target = pr.target_branch.as_deref().filter(|b| !b.is_empty());
    if branch.is_some() || target.is_some() {
        println!(
            "Branch: {} -> {}",
            branch.unwrap_or("?"),
            target.unwrap_or("?")
        );
    }
    if let Some(url) = pr.url.as_deref().filter(|u| !u.is_empty()) {
        println!("URL: {}", ui::terminal_link(url, url));
    }

    let c = &result.comments;
    let has = if c.has_comments {
        format!("{} yes", symbols::ok())
    } else {
        format!("{} no", symbols::neutral())
    };
    println!(
        "Comments: {has} ({} comments in {} threads)",
        c.comment_count, c.thread_count
    );
    let approved = if result.approval.approved {
        format!("{} yes", symbols::ok())
    } else {
        format!("{} no", symbols::fail())
    };
    println!("Approved: {approved} ({})", result.approval.summary);
    let checks = &result.checks;
    println!(
        "Checks: {} ({} passed, {} failed, {} pending)",
        checks.status.as_str().to_uppercase(),
        checks.passed,
        checks.failed,
        checks.pending
    );

    let non_passing: Vec<&CheckDetail> = checks
        .details
        .iter()
        .filter(|d| d.status != CheckStatus::Pass)
        .collect();
    if !non_passing.is_empty() {
        println!("Check details:");
        for check in non_passing {
            let marker = match check.status {
                CheckStatus::Fail => symbols::fail(),
                CheckStatus::Pending => symbols::pending(),
                _ => symbols::neutral(),
            };
            println!(
                "{marker} {}: {}{}",
                check.status.as_str().to_uppercase(),
                check.name,
                link_suffix(check.url.as_deref())
            );
        }
    }

    if result.ready_to_merge {
        ui::print_info("Ready to merge: yes", None);
        return;
    }
    println!("{} Ready to merge: no", symbols::fail());
    println!("Why not ready:");
    for reason in &result.not_ready_reasons {
        let details = reason
            .details
            .as_deref()
            .filter(|d| !d.is_empty())
            .map(|d| format!(" ({d})"))
            .unwrap_or_default();
        println!(
            "{} {}{details}{}",
            symbols::arrow(),
            reason.summary,
            link_suffix(reason.url.as_deref())
        );
    }
}
