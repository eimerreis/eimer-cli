//! `task start` / `task close`.

use anyhow::{Context, Result};
use clap::Args;

use super::work_items::{TaskEnv, WorkItem, resolve_id, state_label};
use crate::core::process::run_json;
use crate::core::ui;

#[derive(Args)]
pub struct StartArgs {
    /// Task ID (prompted when omitted)
    id: Option<u64>,
    /// Print machine-readable JSON
    #[arg(long, short = 'j')]
    json: bool,
}

#[derive(Args)]
pub struct CloseArgs {
    /// Task ID (prompted when omitted)
    id: Option<u64>,
    /// Completed work hours (also sets remaining to 0)
    #[arg(long, short = 'c')]
    completed: Option<f64>,
    /// Print machine-readable JSON
    #[arg(long, short = 'j')]
    json: bool,
}

fn update_state(env: &TaskEnv, id: u64, state: &str, fields: &[String]) -> Result<WorkItem> {
    let mut cmd: Vec<String> = ["az", "boards", "work-item", "update", "--id"]
        .map(String::from)
        .to_vec();
    cmd.extend([id.to_string(), "--state".into(), state.into()]);
    cmd.extend(env.scope.org_args());
    cmd.extend(["--output".into(), "json".into()]);
    if !fields.is_empty() {
        cmd.push("--fields".into());
        cmd.extend(fields.iter().cloned());
    }
    run_json(&cmd)
}

fn report(env: &TaskEnv, verb: &str, updated: &WorkItem, json: bool) -> Result<()> {
    if json {
        return ui::print_json(updated);
    }
    let link = ui::terminal_link(&format!("#{}", updated.id), &env.work_item_url(updated.id));
    ui::print_success(&format!("{verb} task {link}."));
    ui::print_info(
        &format!(
            "{} #{} {}",
            state_label(&updated.state()),
            updated.id,
            updated.title()
        ),
        None,
    );
    Ok(())
}

pub fn start(ctx: Option<&str>, args: StartArgs) -> Result<()> {
    (|| -> Result<()> {
        let env = TaskEnv::init(ctx, args.json)?;
        let id = resolve_id(args.id, "task start [id]")?;
        let updated = ui::spin_unless(args.json, &format!("Starting task #{id}"), || {
            update_state(&env, id, "Active", &[])
        })?;
        report(&env, "Started", &updated, args.json)
    })()
    .context("Failed to start task")
}

pub fn close(ctx: Option<&str>, args: CloseArgs) -> Result<()> {
    (|| -> Result<()> {
        let env = TaskEnv::init(ctx, args.json)?;
        let id = resolve_id(args.id, "task close [id]")?;
        let fields = match args.completed {
            Some(hours) => vec![
                format!("Microsoft.VSTS.Scheduling.CompletedWork={hours}"),
                "Microsoft.VSTS.Scheduling.RemainingWork=0".to_string(),
            ],
            None => vec![],
        };
        let updated = ui::spin_unless(args.json, &format!("Closing task #{id}"), || {
            update_state(&env, id, "Closed", &fields)
        })?;
        report(&env, "Closed", &updated, args.json)
    })()
    .context("Failed to close task")
}
