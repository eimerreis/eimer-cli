//! `eimer time`: record work to codeunity time.
//!
//! Every PR adds a pending entry (`.time/*.md`, via `eimer time add`) or states
//! `Time: none (reason)` in its body; a required CI check blocks merging otherwise.
//! `eimer time book` books pending entries and appends them to `TIME.md`.
//! The pre-push hook (`eimer time hook install`, also run by `init`) writes and
//! updates the branch's entry from measured focus time.

mod activity;
mod api;
mod auth;
mod block;
mod book;
mod duration;
mod hook;
mod loopback;
mod pending;
mod repo;
mod store;
mod suggest;

use anyhow::Result;
use clap::{Args, Subcommand};

#[derive(Args)]
pub struct TimeArgs {
    #[command(subcommand)]
    command: TimeCommand,
}

#[derive(Subcommand)]
enum TimeCommand {
    /// Log in to codeunity time (interactive)
    Login,
    /// Remove the stored codeunity token
    Logout,
    /// List open time projects
    Projects(book::ProjectsArgs),
    /// Add a time project to this repo's .time.json
    Init(book::InitArgs),
    /// Suggest a duration from agent activity (Claude Code, Codex) in this repo
    Suggest(book::SuggestArgs),
    /// Record a pending time entry in .time/
    Add(book::AddArgs),
    /// Manage the git pre-push hook that records focus time automatically
    Hook(hook::HookArgs),
    /// List pending entries and the last booking
    Pending(book::PendingArgs),
    /// Book pending entries to time and append them to TIME.md
    Book(book::BookArgs),
    /// Mark the PR as not billable
    None(book::NoneArgs),
    /// Check the PR adds a time entry or has a Time line (exit 1 otherwise)
    Status(book::StatusArgs),
}

pub fn run(args: TimeArgs) -> Result<()> {
    match args.command {
        TimeCommand::Login => {
            auth::login()?;
            crate::core::ui::print_success("Logged in to codeunity time");
            Ok(())
        }
        TimeCommand::Logout => {
            auth::logout()?;
            crate::core::ui::print_success("Logged out of codeunity time");
            Ok(())
        }
        TimeCommand::Projects(a) => book::projects(a),
        TimeCommand::Init(a) => book::init(a),
        TimeCommand::Suggest(a) => book::suggest(a),
        TimeCommand::Add(a) => book::add(a),
        TimeCommand::Hook(a) => hook::run(a),
        TimeCommand::Pending(a) => book::pending(a),
        TimeCommand::Book(a) => book::book(a),
        TimeCommand::None(a) => book::none(a),
        TimeCommand::Status(a) => book::status(a),
    }
}
