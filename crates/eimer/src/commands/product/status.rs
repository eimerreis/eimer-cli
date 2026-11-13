//! `eimer product status`: changes not covered by an update post yet.

use anyhow::Result;
use clap::Args;

use super::changes::{self, Change, SystemGit};
use super::config;
use crate::core::ui;

#[derive(Args)]
pub struct StatusArgs {
    /// Also list changes that start unchecked (internal commits, excluded products)
    #[arg(long, short = 'a')]
    all: bool,
    /// Print machine-readable JSON
    #[arg(long, short = 'j')]
    json: bool,
}

pub fn run(args: StatusArgs) -> Result<()> {
    let repo = config::load()?;
    let mut collected = changes::collect(&repo, &SystemGit { root: &repo.root })?;
    let internal = collected
        .pending
        .iter()
        .filter(|c| c.internal.is_some())
        .count();
    if !args.all {
        collected.pending.retain(|c| c.internal.is_none());
    }
    if args.json {
        return ui::print_json(&collected);
    }

    let user_facing = collected
        .pending
        .iter()
        .filter(|c| c.internal.is_none())
        .count();
    if collected.pending.is_empty() {
        ui::print_success("Every user-facing change is covered by an update post.");
    } else {
        let rows: Vec<Vec<String>> = collected.pending.iter().map(row).collect();
        println!(
            "{}",
            ui::render_table(
                ["Id", "Date", "Type", "Platforms", "Release", "Change"],
                rows
            )
        );
    }
    let mut parts = vec![format!("{user_facing} to publish")];
    if internal > 0 && !args.all {
        parts.push(format!("{internal} internal (--all)"));
    }
    parts.push(format!("{} covered", collected.covered));
    parts.push(format!("{} ignored", collected.ignored));
    println!("{}", ui::dim(&parts.join(" · ")));
    if user_facing > 0 {
        println!("{}", ui::dim("Next: eimer product update"));
    }
    Ok(())
}

fn row(change: &Change) -> Vec<String> {
    let mut summary = change.summary().to_string();
    if summary.chars().count() > 70 {
        summary = summary.chars().take(69).collect::<String>() + "…";
    }
    let cells = vec![
        change.id.clone(),
        change.date.clone().unwrap_or_default(),
        change.kind.clone().unwrap_or_default(),
        change.platforms.join(", "),
        change.releases.join("\n"),
        summary,
    ];
    if change.internal.is_some() {
        cells.iter().map(|c| ui::dim(c)).collect()
    } else {
        cells
    }
}
