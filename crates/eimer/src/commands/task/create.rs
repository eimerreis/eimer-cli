//! `task create`: create a task in the current sprint of the context's team.

use anyhow::{Context, Result, anyhow, bail};
use clap::Args;

use super::work_items::{TaskEnv, WorkItem, state_label};
use crate::core::error::HintExt;
use crate::core::process::run_json;
use crate::core::{prompt, ui};

#[derive(Args)]
pub struct CreateArgs {
    /// Task title (prompted when omitted)
    title: Vec<String>,
    /// Parent work item ID (Story/Epic)
    #[arg(long, short = 'p')]
    parent: Option<u64>,
    /// Team for the current sprint lookup (defaults to the context's team)
    #[arg(long, short = 't')]
    team: Option<String>,
    /// Original estimate hours
    #[arg(long, short = 'e')]
    estimate: Option<f64>,
    /// Remaining work hours
    #[arg(long, short = 'r')]
    remaining: Option<f64>,
    /// Completed work hours
    #[arg(long, short = 'c')]
    completed: Option<f64>,
    /// Print machine-readable JSON
    #[arg(long, short = 'j')]
    json: bool,
}

pub fn run(ctx: Option<&str>, args: CreateArgs) -> Result<()> {
    create(ctx, args)
        .context("Failed to create task")
        .hint("Check your defaults with `eimer configure --show` / `eimer task context show`, or pass explicit options.")
}

fn create(ctx: Option<&str>, args: CreateArgs) -> Result<()> {
    let env = TaskEnv::init(ctx, args.json)?;
    let area_path = env.default_area_path();

    let mut title = args.title.join(" ").trim().to_string();
    if title.is_empty() {
        title = prompt::text("Task title", None, |v| {
            v.trim().is_empty().then(|| "Title is required".to_string())
        })
        .map_err(|e| anyhow!("Missing task title. Usage: task create \"your title\" ({e})"))?;
    }

    let iteration = resolve_iteration(&env, args.team.as_deref())?;
    let estimate = match args.estimate.or(args.remaining) {
        Some(value) => value,
        None => prompt::text("Original estimate (hours)", Some("4"), |v| {
            match v.trim().parse::<f64>() {
                Ok(n) if n >= 0.0 => None,
                _ => Some("Estimate must be a non-negative number".into()),
            }
        })
        .map_err(|e| anyhow!("Missing original estimate. Pass --estimate or --remaining. ({e})"))?
        .parse()?,
    };

    let mut cmd: Vec<String> = [
        "az",
        "boards",
        "work-item",
        "create",
        "--type",
        "Task",
        "--title",
    ]
    .map(String::from)
    .to_vec();
    cmd.extend([
        title.clone(),
        "--area".into(),
        area_path,
        "--iteration".into(),
        iteration,
    ]);
    cmd.extend(env.scope.args());
    cmd.extend(["--output".into(), "json".into(), "--fields".into()]);
    cmd.push(format!(
        "Microsoft.VSTS.Scheduling.OriginalEstimate={estimate}"
    ));
    if let Some(completed) = args.completed {
        cmd.push(format!(
            "Microsoft.VSTS.Scheduling.CompletedWork={completed}"
        ));
    }
    if let Some(remaining) = args.remaining {
        cmd.push(format!(
            "Microsoft.VSTS.Scheduling.RemainingWork={remaining}"
        ));
    }

    let created: WorkItem = ui::spin_unless(args.json, "Creating task", || run_json(&cmd))?;

    if let Some(parent) = args.parent {
        let mut link: Vec<String> = ["az", "boards", "work-item", "relation", "add", "--id"]
            .map(String::from)
            .to_vec();
        link.extend([
            created.id.to_string(),
            "--relation-type".into(),
            "parent".into(),
            "--target-id".into(),
            parent.to_string(),
        ]);
        link.extend(env.scope.org_args());
        link.extend(["--output".into(), "json".into()]);
        ui::spin_unless(
            args.json,
            &format!("Linking task #{} to parent #{parent}", created.id),
            || run_json::<serde_json::Value, _, _>(&link),
        )?;
    }

    if args.json {
        return ui::print_json(&created);
    }

    let url = env.work_item_url(created.id);
    let title = created
        .field_str("System.Title")
        .map(String::from)
        .unwrap_or(title);
    ui::print_success(&format!(
        "Created task {}.",
        ui::terminal_link(&format!("#{}", created.id), &url)
    ));
    let linked = args.parent.map(|p| format!("Linked to parent #{p}."));
    ui::print_info(
        &format!(
            "{} #{} {} | {}",
            state_label(&created.state()),
            created.id,
            title,
            created.assigned_to()
        ),
        linked.as_deref(),
    );
    Ok(())
}

fn resolve_iteration(env: &TaskEnv, team_override: Option<&str>) -> Result<String> {
    let team_override = team_override.map(str::trim).filter(|t| !t.is_empty());
    let default_team = env.default_team();
    let team = team_override
        .map(String::from)
        .unwrap_or_else(|| default_team.clone());
    if let Some(iteration) = env.current_iteration(&team) {
        return Ok(iteration);
    }
    if team_override.is_some() {
        bail!(
            "No active iteration found for team '{team}'. Confirm the team name or re-run with a different --team value."
        );
    }

    let prompted = prompt::text("Azure DevOps team name", Some(&default_team), |v| {
        v.trim().is_empty().then(|| "Team name is required".to_string())
    })
    .map_err(|_| {
        anyhow!("No active iteration found for default team '{default_team}'. Pass --team or set a team on the context.")
    })?;

    env.current_iteration(&prompted).ok_or_else(|| {
        anyhow!("No active iteration found for team '{prompted}'. Confirm the team exists and has a current sprint configured.")
    })
}
