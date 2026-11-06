//! `release approve`: approve pending Azure DevOps pipeline approvals.

use std::collections::HashSet;

use anyhow::{Context, Result, bail};
use clap::Args;
use serde::Serialize;

use super::api::{Approval, approve_approvals, build_run_url, load_pending_approvals};
use super::client::AzureDevOpsClient;
use super::prod_stage::normalize_text;
use crate::core::error::HintExt;
use crate::core::{prompt, ui};

#[derive(Args)]
pub struct ApproveArgs {
    /// Run ID filter (alternative to --run)
    #[arg(value_name = "RUN_ID")]
    positional: Vec<String>,
    /// Pipeline name filter
    #[arg(long, short = 'p')]
    pipeline: Option<String>,
    /// Run ID filter
    #[arg(long, short = 'r', value_parser = clap::value_parser!(u64).range(1..))]
    run: Option<u64>,
    /// Approval ID
    #[arg(long, short = 'i')]
    id: Option<String>,
    /// Approve all matched approvals
    #[arg(long, short = 'a')]
    all: bool,
    /// Approval comment
    #[arg(long, short = 'c')]
    comment: Option<String>,
    /// Print machine-readable JSON
    #[arg(long, short = 'j')]
    json: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct JsonOutput<'a> {
    matched_count: usize,
    approved_count: usize,
    selected_ids: &'a [String],
    comment: &'a str,
    items: &'a [Approval],
}

pub fn run(args: ApproveArgs) -> Result<()> {
    approve(args)
        .context("Failed to approve")
        .hint("Make sure Azure CLI is authenticated and your approval filters match pending items.")
}

/// First positional parsed as a positive integer (invalid values are ignored, like the TS CLI).
fn resolve_id_arg(flag: Option<u64>, positional: &[String]) -> Option<u64> {
    flag.or_else(|| {
        positional
            .first()
            .and_then(|raw| leading_int(raw.trim()))
            .filter(|n| *n > 0)
    })
}

/// `Number.parseInt` semantics: leading digits only.
fn leading_int(raw: &str) -> Option<u64> {
    let digits: String = raw.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}

fn approve(args: ApproveArgs) -> Result<()> {
    let json = args.json;
    let run_id = resolve_id_arg(args.run, &args.positional);
    let client = ui::spin_unless(
        json,
        "Loading Azure DevOps context",
        AzureDevOpsClient::create,
    )?;
    let pending = ui::spin_unless(json, "Loading pending approvals", || {
        load_pending_approvals(&client)
    })?;

    if pending.is_empty() {
        ui::print_info(
            "No pending approvals.",
            Some("Try `release changelog` if you were expecting a newly deployed release."),
        );
        return Ok(());
    }

    let id_filter = args.id.as_deref().map(str::trim).filter(|v| !v.is_empty());
    let pipeline_filter = args
        .pipeline
        .as_deref()
        .map(normalize_text)
        .filter(|v| !v.is_empty());
    let filtered: Vec<Approval> = pending
        .into_iter()
        .filter(|a| id_filter.is_none_or(|id| a.id() == id))
        .filter(|a| run_id.is_none_or(|run| a.run_id() == Some(run)))
        .filter(|a| {
            pipeline_filter
                .as_ref()
                .is_none_or(|p| normalize_text(&a.pipeline_name().unwrap_or_default()) == *p)
        })
        .collect();

    if filtered.is_empty() {
        ui::print_info(
            "No pending approvals match the provided filters.",
            Some("Check --pipeline / --run values or retry without filters."),
        );
        return Ok(());
    }

    let selected_ids = select_approval_ids(&filtered, args.all)?;
    if selected_ids.is_empty() {
        ui::print_info(
            "No approval selected.",
            Some("Pass --id <approvalId> or --all when running non-interactively."),
        );
        return Ok(());
    }

    let comment = args.comment.as_deref().unwrap_or_default().trim();
    let comment = if comment.is_empty() {
        "Approved via release CLI"
    } else {
        comment
    };
    let approved = ui::spin_unless(json, "Approving selected items", || {
        approve_approvals(&client, &selected_ids, comment)
    })?;

    if json {
        return ui::print_json(&JsonOutput {
            matched_count: filtered.len(),
            approved_count: approved.len(),
            selected_ids: &selected_ids,
            comment,
            items: &approved,
        });
    }

    for item in &approved {
        let pipeline_name = item
            .pipeline_name()
            .unwrap_or_else(|| "Unknown pipeline".into());
        let target = match item.run_id().filter(|id| *id > 0) {
            Some(id) => {
                ui::terminal_link(&format!("run #{id}"), &build_run_url(&client.context, id))
            }
            None => "-".into(),
        };
        println!(
            "{}",
            ui::render_table(
                ["Approval", "Pipeline", "Run"],
                [vec![item.id(), pipeline_name, target]]
            )
        );
        ui::print_success(&format!("Approved {}", item.id()));
    }
    if approved.is_empty() {
        ui::print_info("Approval call returned no updated approvals.", None);
    }
    Ok(())
}

fn select_approval_ids(approvals: &[Approval], approve_all: bool) -> Result<Vec<String>> {
    if approvals.len() == 1 || approve_all {
        return Ok(approvals.iter().map(Approval::id).collect());
    }
    if !prompt::is_interactive() {
        return Ok(Vec::new());
    }

    let rows = approvals.iter().take(20).map(|item| {
        vec![
            item.id(),
            item.pipeline_name()
                .unwrap_or_else(|| "Unknown pipeline".into()),
            item.run_id()
                .filter(|id| *id > 0)
                .map_or_else(|| "-".into(), |id| format!("#{id}")),
            item.created_on()
                .map_or_else(|| "unknown-time".into(), |c| to_iso_string(&c)),
        ]
    });
    let table = ui::render_table(["Approval", "Pipeline", "Run", "Created"], rows);
    let raw = prompt::text(
        &format!("Approval IDs to approve (comma-separated):\n{table}"),
        None,
        |v| {
            v.trim()
                .is_empty()
                .then(|| "At least one approval ID is required".to_string())
        },
    )?;
    parse_selection(&raw, approvals)
}

fn parse_selection(raw: &str, approvals: &[Approval]) -> Result<Vec<String>> {
    let selected: Vec<&str> = raw
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    let known: HashSet<String> = approvals.iter().map(Approval::id).collect();
    let unknown: Vec<&str> = selected
        .iter()
        .copied()
        .filter(|s| !known.contains(*s))
        .collect();
    if !unknown.is_empty() {
        bail!("Unknown approval ID(s): {}", unknown.join(", "));
    }
    let mut seen = HashSet::new();
    Ok(selected
        .into_iter()
        .filter(|s| seen.insert(*s))
        .map(String::from)
        .collect())
}

/// `new Date(value).toISOString()`: UTC with millisecond precision.
fn to_iso_string(value: &str) -> String {
    chrono::DateTime::parse_from_rfc3339(value)
        .map(|d| {
            d.with_timezone(&chrono::Utc)
                .format("%Y-%m-%dT%H:%M:%S%.3fZ")
                .to_string()
        })
        .unwrap_or_else(|_| value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn approvals() -> Vec<Approval> {
        vec![Approval(json!({"id": "a1"})), Approval(json!({"id": "b2"}))]
    }

    #[test]
    fn parses_selection() {
        assert_eq!(
            parse_selection(" b2, a1 ,b2,", &approvals()).unwrap(),
            vec!["b2", "a1"]
        );
        assert_eq!(
            parse_selection("a1,zz", &approvals())
                .unwrap_err()
                .to_string(),
            "Unknown approval ID(s): zz"
        );
    }

    #[test]
    fn id_args_and_dates() {
        assert_eq!(resolve_id_arg(Some(5), &["9".into()]), Some(5));
        assert_eq!(resolve_id_arg(None, &["12abc".into()]), Some(12));
        assert_eq!(resolve_id_arg(None, &["abc".into()]), None);
        assert_eq!(resolve_id_arg(None, &["0".into()]), None);
        assert_eq!(
            to_iso_string("2024-05-01T10:00:00.1234567+02:00"),
            "2024-05-01T08:00:00.123Z"
        );
        let a = Approval(json!({"id": "x", "pipeline": {"name": "App", "owner": {"id": 42}}}));
        assert_eq!(
            (a.pipeline_name().as_deref(), a.run_id()),
            (Some("App"), Some(42))
        );
    }
}
