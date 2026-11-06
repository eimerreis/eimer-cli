//! `task show`: a task and its parent hierarchy (Task -> Story -> Epic).

use std::collections::HashSet;

use anyhow::{Context, Result, bail};
use clap::Args;
use serde::Serialize;

use super::work_items::{TaskEnv, resolve_id, state_label};
use crate::core::ui;

#[derive(Args)]
pub struct ShowArgs {
    /// Task ID (prompted when omitted)
    id: Option<u64>,
    /// Do not load the parent hierarchy
    #[arg(long)]
    no_parents: bool,
    /// Allow non-task root items (Bug/Story/etc)
    #[arg(long, short = 'n')]
    allow_non_task: bool,
    /// Print machine-readable JSON
    #[arg(long, short = 'j')]
    json: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ShowNode {
    id: u64,
    title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    state: String,
    #[serde(rename = "type")]
    item_type: String,
    assigned_to: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    changed_date: Option<String>,
}

pub fn run(ctx: Option<&str>, args: ShowArgs) -> Result<()> {
    (|| -> Result<()> {
        let env = TaskEnv::init(ctx, args.json)?;
        let id = resolve_id(args.id, "task show [id]")?;
        let chain = ui::spin_unless(args.json, "Loading task details", || {
            load_chain(&env, id, !args.no_parents)
        })?;

        if let Some(root) = chain.first()
            && !args.allow_non_task
            && !root.item_type.eq_ignore_ascii_case("task")
        {
            bail!(
                "Work item #{} is '{}', not 'Task'. Pass --allow-non-task to show it anyway.",
                root.id,
                root.item_type
            );
        }

        if args.json {
            return ui::print_json(&chain);
        }
        print_hierarchy(&env, &chain);
        Ok(())
    })()
    .context("Failed to show task")
}

fn load_chain(env: &TaskEnv, id: u64, with_parents: bool) -> Result<Vec<ShowNode>> {
    let mut chain = Vec::new();
    let mut visited = HashSet::new();
    let mut current = Some(id);

    while let Some(id) = current.filter(|id| visited.insert(*id)) {
        let wi = env.show_work_item(id, true)?;
        chain.push(ShowNode {
            id,
            title: wi.title(),
            description: wi.description(),
            state: wi.state(),
            item_type: wi.item_type(),
            assigned_to: wi.assigned_to(),
            changed_date: wi.field_str("System.ChangedDate").map(String::from),
        });
        current = if with_parents { wi.parent_id() } else { None };
    }
    Ok(chain)
}

fn print_hierarchy(env: &TaskEnv, chain: &[ShowNode]) {
    let Some(root) = chain.first() else {
        ui::print_info("No task data found.", None);
        return;
    };
    println!(
        "{}: {}",
        ui::bold("Task"),
        ui::terminal_link(&format!("#{}", root.id), &env.work_item_url(root.id))
    );

    for (index, item) in chain.iter().enumerate() {
        let indent = "  ".repeat(index);
        let connector = if index == 0 { "" } else { "`- " };
        let changed = ui::relative_time(item.changed_date.as_deref());
        let changed = if changed.is_empty() {
            String::new()
        } else {
            format!(" | {changed}")
        };
        let line = format!(
            "{indent}{connector}{} {} #{} {} | {}{changed}",
            item.item_type.to_uppercase(),
            state_label(&item.state),
            item.id,
            item.title,
            item.assigned_to
        );
        println!("{}", ui::terminal_link(&line, &env.work_item_url(item.id)));
    }
}
