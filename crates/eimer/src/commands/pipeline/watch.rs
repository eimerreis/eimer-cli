//! `pipeline watch [id]`: poll a run until it completes. Exits 1 unless it succeeded.

use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use clap::Args;
use serde_json::Value;

use super::common::{Run, az_json, get_repo_info, latest_repo_run_id, resolve_id_arg};
use crate::core::error::{HintExt, SilentExit};
use crate::core::process::run_json;
use crate::core::ui::{self, symbols};

#[derive(Args)]
pub struct WatchArgs {
    /// Run ID (defaults to latest run in this repo)
    #[arg(value_name = "ID")]
    run: Option<String>,
    /// Run ID (defaults to latest run in this repo)
    #[arg(long, short = 'i', value_parser = clap::value_parser!(u64).range(1..))]
    id: Option<u64>,
    /// Polling interval in seconds
    #[arg(long, short = 't', default_value_t = 15, value_parser = clap::value_parser!(u64).range(1..))]
    interval: u64,
    /// Print machine-readable JSON on completion (progress goes to stderr)
    #[arg(long, short = 'j')]
    json: bool,
}

pub fn run(args: WatchArgs) -> Result<()> {
    match watch(args) {
        Err(err) if err.downcast_ref::<SilentExit>().is_some() => Err(err),
        result => result
            .hint("Make sure Azure CLI is authenticated and the run belongs to the configured project.")
            .context("Failed to watch pipeline run"),
    }
}

/// Progress output: stdout normally, stderr with `--json` so stdout stays parseable.
struct Progress {
    json: bool,
}

impl Progress {
    fn line(&self, line: &str) {
        if self.json {
            eprintln!("{line}");
        } else {
            println!("{line}");
        }
    }
}

fn watch(args: WatchArgs) -> Result<()> {
    let run_id = match resolve_id_arg(args.id, args.run.as_deref()) {
        Some(id) => Some(id),
        None => ui::spin_unless(args.json, "Finding latest pipeline run", || {
            latest_repo_run_id(&get_repo_info()?.name)
        })?,
    }
    .ok_or_else(|| {
        anyhow!(
            "Missing run ID. Usage: pipeline watch [id]. Try 'pipeline runs' first to pick a run."
        )
    })?;

    let progress = Progress { json: args.json };
    progress.line(&format!(
        "{} Watching pipeline run #{run_id}.",
        symbols::info()
    ));
    progress.line(&ui::dim(&format!(
        "Hint: Polling every {}s.",
        args.interval
    )));

    let id = run_id.to_string();
    let cmd = az_json(&["pipelines", "runs", "show", "--id", &id]);
    let mut last_line = String::new();
    loop {
        let value: Value = run_json(&cmd)?;
        let run = Run(&value);
        let line = status_line(run);
        if line != last_line {
            progress.line(&line);
            last_line = line;
        }

        if run
            .status()
            .is_some_and(|s| s.eq_ignore_ascii_case("completed"))
        {
            let succeeded = run
                .result()
                .is_some_and(|r| r.eq_ignore_ascii_case("succeeded"));
            if args.json {
                ui::print_json(&value)?;
            } else if succeeded {
                ui::print_success(&format!(
                    "Pipeline run #{} completed successfully.",
                    run.id()
                ));
            } else {
                ui::print_error(
                    &format!(
                        "Pipeline run #{} completed with result '{}'.",
                        run.id(),
                        run.result().unwrap_or("unknown")
                    ),
                    None,
                );
            }
            return if succeeded {
                Ok(())
            } else {
                Err(SilentExit(1).into())
            };
        }

        std::thread::sleep(Duration::from_secs(args.interval));
    }
}

/// "<state> #<id> <pipeline> | <message> | <duration>"
fn status_line(run: Run) -> String {
    format!(
        "{} {} {} | {} | {}",
        run.status_label(),
        run.link(),
        run.pipeline_name(),
        run.message(),
        run.duration()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn formats_status_line() {
        let v = json!({
            "id": 7, "status": "completed", "result": "failed",
            "definition": {"name": "ci"}, "triggerInfo": {"ci.message": "msg"},
            "startTime": "2024-01-01T10:00:00Z", "finishTime": "2024-01-01T10:01:02Z"
        });
        assert_eq!(status_line(Run(&v)), "X FAILED #7 ci | msg | 1m 2s");
    }
}
