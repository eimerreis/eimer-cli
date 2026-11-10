//! `eimer time`: book PR work to codeunity time and record it in the PR body.
//!
//! Every PR gets a `Time:` line (`Time: 1.5h → time#123` or `Time: none (reason)`);
//! a required CI check blocks merging without one.

mod api;
mod auth;
mod block;
mod book;
mod duration;
mod loopback;
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
    /// Suggest a duration from the PR's commit times
    Suggest(book::SuggestArgs),
    /// Create or update the PR's time record and write its Time line
    Book(book::BookArgs),
    /// Mark the PR as not billable
    None(book::NoneArgs),
    /// Show the PR's Time line (exit 1 when missing or invalid)
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
        TimeCommand::Book(a) => book::book(a),
        TimeCommand::None(a) => book::none(a),
        TimeCommand::Status(a) => book::status(a),
    }
}
