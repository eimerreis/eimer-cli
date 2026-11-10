//! `pipeline runs`: recent runs for the current repository.

use anyhow::{Context, Result};
use clap::Args;
use serde_json::{Value, json};

use super::common::{Run, az_json, get_repo_info};
use crate::core::process::run_json;
use crate::core::ui;

#[derive(Args)]
pub struct RunsArgs {
    /// Maximum number of runs to print
    #[arg(long, short = 'n', default_value_t = 10, value_parser = clap::value_parser!(u64).range(1..))]
    top: u64,
    /// Include runs from all repositories
    #[arg(long, short = 'a')]
    all: bool,
    /// Filter by branch
    #[arg(long, short = 'b')]
    branch: Option<String>,
    /// Filter by result (succeeded/failed/canceled)
    #[arg(long, short = 'r')]
    result: Option<String>,
    /// Filter by status (inProgress/completed/notStarted)
    #[arg(long, short = 's')]
    status: Option<String>,
    /// Print machine-readable JSON
    #[arg(long, short = 'j')]
    json: bool,
}

/// Without `--all`, over-fetch so enough runs remain after filtering by repository.
fn fetch_top(top: u64, all: bool) -> u64 {
    if all { top } else { (top * 5).clamp(50, 200) }
}

pub fn run(args: RunsArgs) -> Result<()> {
    list_runs(args).context("Failed to list pipeline runs")
}

fn list_runs(args: RunsArgs) -> Result<()> {
    let repo = ui::spin_unless(args.json, "Detecting repository", get_repo_info)?;

    let top = fetch_top(args.top, args.all).to_string();
    let mut cmd = az_json(&[
        "pipelines",
        "runs",
        "list",
        "--top",
        &top,
        "--query-order",
        "QueueTimeDesc",
    ]);
    let filters = [
        ("--branch", &args.branch),
        ("--status", &args.status),
        ("--result", &args.result),
    ];
    for (flag, value) in filters {
        if let Some(value) = value.as_deref().map(str::trim).filter(|v| !v.is_empty()) {
            cmd.extend([flag.to_string(), value.to_string()]);
        }
    }

    let all_runs: Vec<Value> =
        ui::spin_unless(args.json, "Loading pipeline runs", || run_json(&cmd))?;
    let runs: Vec<&Value> = all_runs
        .iter()
        .filter(|r| args.all || Run(r).matches_repo(&repo.name))
        .take(args.top as usize)
        .collect();

    if args.json {
        return ui::print_json(&json!({
            "repository": repo,
            "count": runs.len(),
            "items": runs,
        }));
    }

    if runs.is_empty() {
        if args.all {
            ui::print_info("No pipeline runs found for all repos.", None);
        } else {
            ui::print_info(
                &format!("No pipeline runs found for repo '{}'.", repo.name),
                Some("Try --all to include other repositories."),
            );
        }
        return Ok(());
    }

    let rows = runs.iter().map(|value| {
        let run = Run(value);
        let queued = ui::relative_time(run.str(&["queueTime"]));
        vec![
            run.status_label(),
            run.link(),
            run.pipeline_name().to_string(),
            run.message(),
            run.duration(),
            if queued.is_empty() {
                "-".into()
            } else {
                queued
            },
        ]
    });
    println!(
        "{}",
        ui::render_table(
            ["State", "Run", "Pipeline", "Message", "Duration", "Queued"],
            rows
        )
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::fetch_top;

    #[test]
    fn over_fetches_for_repo_filter() {
        assert_eq!(fetch_top(10, true), 10);
        assert_eq!(fetch_top(10, false), 50);
        assert_eq!(fetch_top(20, false), 100);
        assert_eq!(fetch_top(100, false), 200);
    }
}
