//! `pipeline open [id]`: open a run in the browser (latest run for the repo by default).

use anyhow::{Context, Result, anyhow};
use clap::Args;
use serde_json::json;

use super::common::{get_repo_info, latest_repo_run_id, resolve_id_arg};
use crate::core::azure::AzScope;
use crate::core::error::HintExt;
use crate::core::process::run_text;
use crate::core::ui;

#[derive(Args)]
pub struct OpenArgs {
    /// Run ID (opens latest for repo when omitted)
    #[arg(value_name = "ID")]
    run: Option<String>,
    /// Run ID (opens latest for repo when omitted)
    #[arg(long, short = 'i', value_parser = clap::value_parser!(u64).range(1..))]
    id: Option<u64>,
    /// Print machine-readable JSON
    #[arg(long, short = 'j')]
    json: bool,
}

pub fn run(args: OpenArgs) -> Result<()> {
    open(args)
        .hint("Make sure Azure CLI is authenticated and the run exists in the configured project.")
        .context("Failed to open pipeline run")
}

fn open(args: OpenArgs) -> Result<()> {
    let run_id = match resolve_id_arg(args.id, args.run.as_deref()) {
        Some(id) => id,
        None => {
            let repo = ui::spin_unless(args.json, "Detecting repository", get_repo_info)?;
            ui::spin_unless(args.json, "Loading recent pipeline runs", || {
                latest_repo_run_id(&repo.name)
            })?
            .ok_or_else(|| {
                anyhow!(
                    "No pipeline runs found for repo '{}'. Try 'pipeline runs --all' or pass --id.",
                    repo.name
                )
            })?
        }
    };

    let id = run_id.to_string();
    let mut cmd: Vec<String> = ["az", "pipelines", "runs", "show", "--id", &id, "--open"]
        .map(String::from)
        .to_vec();
    cmd.extend(AzScope::detect().args());
    ui::spin_unless(
        args.json,
        &format!("Opening pipeline run #{run_id}"),
        || run_text(&cmd),
    )?;

    if args.json {
        return ui::print_json(&json!({ "opened": true, "runId": run_id }));
    }
    ui::print_success(&format!("Opened pipeline run #{run_id}."));
    Ok(())
}
