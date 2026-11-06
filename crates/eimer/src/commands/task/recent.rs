//! `task recent`: your recently changed tasks.

use anyhow::{Context, Result};
use clap::Args;

use super::work_items::{LIST_FIELDS, TaskEnv, print_items};
use crate::core::ui;

#[derive(Args)]
pub struct RecentArgs {
    /// Include closed and non-active states
    #[arg(long, short = 'a')]
    all: bool,
    /// Maximum number of tasks to print
    #[arg(long, short = 't', default_value_t = 10)]
    top: usize,
    /// Print machine-readable JSON
    #[arg(long, short = 'j')]
    json: bool,
}

pub fn run(ctx: Option<&str>, args: RecentArgs) -> Result<()> {
    (|| -> Result<()> {
        let env = TaskEnv::init(ctx, args.json)?;
        let wiql = format!(
            "SELECT {LIST_FIELDS} FROM WorkItems WHERE [System.AssignedTo] = @Me AND [System.WorkItemType] = 'Task' ORDER BY [System.ChangedDate] DESC"
        );
        let rows = ui::spin_unless(args.json, "Loading recent tasks", || env.query(&wiql))?;
        let top: Vec<_> = rows
            .into_iter()
            .filter(|wi| args.all || wi.state().eq_ignore_ascii_case("active"))
            .take(args.top)
            .collect();

        if args.json {
            return ui::print_json(&top);
        }
        if top.is_empty() {
            let hint = (!args.all).then_some("Try --all to include closed and non-active tasks.");
            ui::print_info("No recent tasks found.", hint);
            return Ok(());
        }
        let items: Vec<_> = top.iter().map(|wi| wi.to_list_item()).collect();
        print_items(&env, &items);
        Ok(())
    })()
    .context("Failed to list recent tasks")
}
