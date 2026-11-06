//! `eimer task`: Azure DevOps task workflows, scoped to the active context.

mod context;
mod create;
mod list;
mod recent;
mod show;
mod update;
mod work_items;

use anyhow::Result;
use clap::{Args, Subcommand};

#[derive(Args)]
pub struct TaskArgs {
    /// Context from config.json to act in (overrides EIMER_CONTEXT and .eimerrc)
    #[arg(long, global = true, value_name = "NAME")]
    context: Option<String>,

    #[command(subcommand)]
    command: TaskCommand,
}

#[derive(Subcommand)]
enum TaskCommand {
    /// Create a task in the current sprint
    Create(create::CreateArgs),
    /// List tasks (defaults to `list mine`)
    List(list::ListArgs),
    /// List your recently changed tasks
    Recent(recent::RecentArgs),
    /// Set a task to Active
    Start(update::StartArgs),
    /// Close a task
    Close(update::CloseArgs),
    /// Show a task and its parent hierarchy
    Show(show::ShowArgs),
    /// Manage the context (org/project) task commands act in
    Context(context::ContextArgs),
}

pub fn run(args: TaskArgs) -> Result<()> {
    let ctx = args.context.as_deref();
    match args.command {
        TaskCommand::Create(a) => create::run(ctx, a),
        TaskCommand::List(a) => list::run(ctx, a),
        TaskCommand::Recent(a) => recent::run(ctx, a),
        TaskCommand::Start(a) => update::start(ctx, a),
        TaskCommand::Close(a) => update::close(ctx, a),
        TaskCommand::Show(a) => show::run(ctx, a),
        TaskCommand::Context(a) => context::run(ctx, a),
    }
}
