//! `task list [mine|team|children]`. Bare `task list` runs `mine`;
//! `task list <id>` and `--parent <id>` are kept as shortcuts for `children`.

use anyhow::{Context, Result, anyhow};
use clap::{Args, Subcommand};
use serde_json::json;

use super::work_items::{
    LIST_FIELDS, ListItem, NOT_STARTED_STATES, TaskEnv, print_items, state_label,
};
use crate::core::azure::escape_wiql;
use crate::core::ui;

#[derive(Args)]
#[command(args_conflicts_with_subcommands = true)]
pub struct ListArgs {
    #[command(subcommand)]
    command: Option<ListCommand>,

    #[command(flatten)]
    mine: MineArgs,

    /// Deprecated: use `task list children <id>`
    #[arg(long, short = 'p', hide = true)]
    parent: Option<u64>,

    /// Parent work item ID (shortcut for `task list children <id>`)
    #[arg(hide = true)]
    id: Option<u64>,
}

#[derive(Subcommand)]
enum ListCommand {
    /// Not-started tasks assigned to me in the current sprint
    Mine(MineArgs),
    /// Active tasks in the team's current sprint
    Team(TeamArgs),
    /// Child tasks under a parent work item
    Children(ChildrenArgs),
}

#[derive(Args, Clone)]
struct Common {
    /// Maximum number of tasks to print
    #[arg(long, short = 'n', default_value_t = 50)]
    top: usize,
    /// Print machine-readable JSON
    #[arg(long, short = 'j')]
    json: bool,
}

#[derive(Args, Clone)]
struct MineArgs {
    /// Team for the current sprint lookup (defaults to the context's team)
    #[arg(long, short = 't')]
    team: Option<String>,
    /// Include tasks in any state
    #[arg(long, short = 'a')]
    all: bool,
    #[command(flatten)]
    common: Common,
}

#[derive(Args)]
struct TeamArgs {
    /// Team for the current sprint lookup (defaults to the context's team)
    #[arg(long, short = 't')]
    team: Option<String>,
    /// Include non-active states
    #[arg(long, short = 'a')]
    all: bool,
    #[command(flatten)]
    common: Common,
}

#[derive(Args)]
struct ChildrenArgs {
    /// Parent work item ID
    id: u64,
    /// Include non-active states
    #[arg(long, short = 'a')]
    all: bool,
    #[command(flatten)]
    common: Common,
}

pub fn run(ctx: Option<&str>, args: ListArgs) -> Result<()> {
    let result = match args.command {
        Some(ListCommand::Mine(a)) => mine(ctx, a),
        Some(ListCommand::Team(a)) => team(ctx, a),
        Some(ListCommand::Children(a)) => children(ctx, a),
        None => match args.parent.or(args.id) {
            Some(id) => children(
                ctx,
                ChildrenArgs {
                    id,
                    all: args.mine.all,
                    common: args.mine.common,
                },
            ),
            None => mine(ctx, args.mine),
        },
    };
    result.context("Failed to list tasks")
}

fn resolve_sprint(env: &TaskEnv, team: Option<String>) -> Result<(String, String)> {
    let team = team
        .filter(|t| !t.trim().is_empty())
        .unwrap_or_else(|| env.default_team());
    let iteration = env.current_iteration(&team).ok_or_else(|| {
        anyhow!(
            "No active iteration found for team '{team}'. Pass --team or set a team on the context."
        )
    })?;
    Ok((team, iteration))
}

fn query_tasks(env: &TaskEnv, filter: &str, order_by: &str) -> Result<Vec<ListItem>> {
    let wiql = format!(
        "SELECT {LIST_FIELDS} FROM WorkItems WHERE [System.WorkItemType] = 'Task' AND {filter} ORDER BY {order_by}"
    );
    Ok(env
        .query(&wiql)?
        .iter()
        .map(|wi| wi.to_list_item())
        .collect())
}

/// "Team: Core · Context: tapio"
fn describe_scope(env: &TaskEnv, team: &str) -> String {
    let mut parts = vec![format!("Team: {team}")];
    if let Some(name) = env.context_name() {
        parts.push(format!("Context: {name}"));
    }
    parts.join(" · ")
}

fn mine(ctx: Option<&str>, args: MineArgs) -> Result<()> {
    let env = TaskEnv::init(ctx, args.common.json)?;
    let (team, iteration, items) =
        ui::spin_unless(args.common.json, "Loading your tasks", || -> Result<_> {
            let (team, iteration) = resolve_sprint(&env, args.team.clone())?;
            let states = NOT_STARTED_STATES
                .iter()
                .map(|s| format!("'{}'", escape_wiql(s)))
                .collect::<Vec<_>>()
                .join(", ");
            let state_filter = if args.all {
                String::new()
            } else {
                format!(" AND [System.State] IN ({states})")
            };
            let filter = format!(
                "[System.AssignedTo] = @Me AND [System.IterationPath] = '{}'{state_filter}",
                escape_wiql(&iteration)
            );
            let mut items = query_tasks(&env, &filter, "[System.ChangedDate] DESC")?;
            items.truncate(args.common.top);
            Ok((team, iteration, items))
        })?;

    if args.common.json {
        return ui::print_json(&json!({
            "context": env.context_name(), "mode": "mine", "team": team, "iterationPath": iteration, "items": items
        }));
    }

    if items.is_empty() {
        let message = format!(
            "No {}tasks assigned to you in '{iteration}'.",
            if args.all { "" } else { "not-started " }
        );
        let hint = if args.all {
            describe_scope(&env, &team)
        } else {
            "Try --all to include tasks in any state.".into()
        };
        ui::print_info(&message, Some(&hint));
        return Ok(());
    }

    ui::print_info(
        &format!("My tasks in {iteration}"),
        Some(&describe_scope(&env, &team)),
    );
    print_items(&env, &items);
    Ok(())
}

fn team(ctx: Option<&str>, args: TeamArgs) -> Result<()> {
    let env = TaskEnv::init(ctx, args.common.json)?;
    let (team, iteration, items) =
        ui::spin_unless(args.common.json, "Loading sprint tasks", || -> Result<_> {
            let (team, iteration) = resolve_sprint(&env, args.team.clone())?;
            let state_filter = if args.all {
                ""
            } else {
                " AND [System.State] = 'Active'"
            };
            let filter = format!(
                "[System.IterationPath] = '{}'{state_filter}",
                escape_wiql(&iteration)
            );
            let mut items = query_tasks(
                &env,
                &filter,
                "[System.AssignedTo] ASC, [System.ChangedDate] DESC",
            )?;
            items.truncate(args.common.top);
            Ok((team, iteration, items))
        })?;

    if args.common.json {
        return ui::print_json(&json!({
            "context": env.context_name(), "mode": "sprint", "team": team, "iterationPath": iteration, "items": items
        }));
    }

    if items.is_empty() {
        let message = format!(
            "No {} found for '{iteration}'.",
            if args.all { "tasks" } else { "active tasks" }
        );
        let hint = if args.all {
            describe_scope(&env, &team)
        } else {
            "Try --all to include non-active states.".into()
        };
        ui::print_info(&message, Some(&hint));
        return Ok(());
    }

    ui::print_info(
        &format!("Sprint {iteration}"),
        Some(&describe_scope(&env, &team)),
    );
    print_items(&env, &items);
    Ok(())
}

fn children(ctx: Option<&str>, args: ChildrenArgs) -> Result<()> {
    let env = TaskEnv::init(ctx, args.common.json)?;
    let (parent, items) =
        ui::spin_unless(args.common.json, "Loading child tasks", || -> Result<_> {
            let parent = env.show_work_item(args.id, true)?;
            let env = &env;
            let children: Vec<ListItem> = std::thread::scope(|s| {
                let handles: Vec<_> = parent
                    .child_ids()
                    .into_iter()
                    .map(|id| s.spawn(move || env.show_work_item(id, false)))
                    .collect();
                handles
                    .into_iter()
                    .map(|h| h.join().map_err(|_| anyhow!("worker thread panicked"))?)
                    .collect::<Result<Vec<_>>>()
            })?
            .iter()
            .map(|wi| wi.to_list_item())
            .filter(|item| item.item_type.eq_ignore_ascii_case("task"))
            .filter(|item| args.all || item.state.eq_ignore_ascii_case("active"))
            .take(args.common.top)
            .collect();
            Ok((parent.to_list_item(), children))
        })?;

    if args.common.json {
        return ui::print_json(&json!({
            "context": env.context_name(), "mode": "parent", "parent": parent, "items": items
        }));
    }

    let line = format!(
        "PARENT {} {} #{} {} | {}",
        parent.item_type.to_uppercase(),
        state_label(&parent.state),
        parent.id,
        parent.title,
        parent.assigned_to
    );
    println!(
        "{}",
        ui::terminal_link(&line, &env.work_item_url(parent.id))
    );

    if items.is_empty() {
        let (message, hint) = if args.all {
            ("No task children found.", None)
        } else {
            (
                "No active task children found.",
                Some("Try --all to include non-active child tasks."),
            )
        };
        ui::print_info(message, hint);
        return Ok(());
    }

    print_items(&env, &items);
    Ok(())
}
