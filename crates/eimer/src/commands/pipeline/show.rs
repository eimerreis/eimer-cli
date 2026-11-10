//! `pipeline show [id]`: details for a single run.

use anyhow::{Context, Result, anyhow};
use clap::Args;
use serde_json::Value;

use super::common::{Run, az_json, build_run_url, resolve_id_arg, strip_branch_prefix};
use crate::core::process::run_json;
use crate::core::{prompt, ui};

#[derive(Args)]
pub struct ShowArgs {
    /// Pipeline run ID (prompted when omitted)
    #[arg(value_name = "ID")]
    run: Option<String>,
    /// Pipeline run ID
    #[arg(long, short = 'i', value_parser = clap::value_parser!(u64).range(1..))]
    id: Option<u64>,
    /// Print machine-readable JSON
    #[arg(long, short = 'j')]
    json: bool,
}

pub fn run(args: ShowArgs) -> Result<()> {
    show(args).context("Failed to show pipeline run")
}

fn show(args: ShowArgs) -> Result<()> {
    let run_id = match resolve_id_arg(args.id, args.run.as_deref()) {
        Some(id) => id,
        None => prompt::id("Pipeline run ID")
            .map_err(|e| anyhow!("Missing run ID. Usage: pipeline show [id] ({e})"))?,
    };

    let id = run_id.to_string();
    let value: Value = ui::spin_unless(
        args.json,
        &format!("Loading pipeline run #{run_id}"),
        || run_json(az_json(&["pipelines", "runs", "show", "--id", &id])),
    )?;

    if args.json {
        return ui::print_json(&value);
    }

    let run = Run(&value);
    let branch = match strip_branch_prefix(run.str(&["sourceBranch"])) {
        "" => "unknown",
        b => b,
    };
    let requested_by = run
        .str(&["requestedFor", "displayName"])
        .or_else(|| run.str(&["requestedFor", "uniqueName"]))
        .unwrap_or("unknown");
    let queued = ui::relative_time(run.str(&["queueTime"]));
    let field = |path: &str| run.str(&[path]).unwrap_or("unknown").to_string();

    println!(
        "{}",
        ui::terminal_link(
            &format!("{} #{}", ui::bold(run.pipeline_name()), run.id()),
            &build_run_url(run.id())
        )
    );
    let rows: Vec<Vec<String>> = vec![
        vec!["State".into(), run.status_label()],
        vec!["Message".into(), run.message()],
        vec!["Branch".into(), branch.into()],
        vec!["By".into(), requested_by.into()],
        vec!["Reason".into(), field("reason")],
        vec![
            "Queued".into(),
            if queued.is_empty() {
                "unknown".into()
            } else {
                queued
            },
        ],
        vec!["Duration".into(), run.duration()],
        vec!["Commit".into(), field("sourceVersion")],
    ];
    println!("{}", ui::render_table(["Field", "Value"], rows));
    Ok(())
}
