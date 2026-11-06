//! `eimer pipeline`: Azure Pipelines workflows. Org/project come from the git remote
//! (`az ... --detect true`); pipeline commands do not use contexts.

mod common;
mod list;
mod open;
mod repo;
mod runs;
mod show;
mod trigger;
mod watch;

use anyhow::Result;
use clap::{Args, Subcommand};

#[derive(Args)]
pub struct PipelineArgs {
    #[command(subcommand)]
    command: PipelineCommand,
}

#[derive(Subcommand)]
enum PipelineCommand {
    /// List recent pipeline runs for this repository
    Runs(runs::RunsArgs),
    /// List pipelines, optionally filtered by glob
    List(list::ListArgs),
    /// Open a pipeline run in browser
    Open(open::OpenArgs),
    /// Show details for a pipeline run
    Show(show::ShowArgs),
    /// Queue a new pipeline run
    Trigger(trigger::TriggerArgs),
    /// Watch a pipeline run until completion
    Watch(watch::WatchArgs),
}

pub fn run(args: PipelineArgs) -> Result<()> {
    match args.command {
        PipelineCommand::Runs(a) => runs::run(a),
        PipelineCommand::List(a) => list::run(a),
        PipelineCommand::Open(a) => open::run(a),
        PipelineCommand::Show(a) => show::run(a),
        PipelineCommand::Trigger(a) => trigger::run(a),
        PipelineCommand::Watch(a) => watch::run(a),
    }
}
