//! `eimer doc`: business documents (invoice, quote, letter, reminder, contract, time report)
//! in the EimerReis brand, rendered from YAML with Typst. Templates and fonts are embedded;
//! the sender's business details live in `~/.config/eimer/sender.yaml`.

pub mod assets;
pub mod render;

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use clap::{Args, Subcommand};

use crate::core::error::HintExt;
use crate::core::{config, ui};

pub use render::{RenderOptions, TYPES};

#[derive(Args)]
pub struct DocArgs {
    #[command(subcommand)]
    command: DocCommand,
}

#[derive(Subcommand)]
enum DocCommand {
    /// Render a document YAML to PDF; its `type:` picks the template
    Render(RenderArgs),
    /// Write an example document to start from, or `sender` for your business details
    New(NewArgs),
}

#[derive(Args)]
struct RenderArgs {
    /// Document YAML
    data: PathBuf,
    /// Output file (default: next to the YAML, with .pdf)
    #[arg(short, long)]
    output: Option<PathBuf>,
    /// Sender details (default: ~/.config/eimer/sender.yaml)
    #[arg(long)]
    sender: Option<PathBuf>,
    /// Re-render on every save
    #[arg(short, long)]
    watch: bool,
    /// Render page previews as PNG instead of a PDF
    #[arg(long)]
    png: bool,
}

#[derive(Args)]
struct NewArgs {
    /// invoice, quote, letter, reminder, contract, time-report, or sender
    #[arg(value_parser = clap::builder::PossibleValuesParser::new(
        TYPES.iter().copied().chain(["sender"])
    ))]
    kind: String,
    /// Target file (default: ./<type>.yaml; for sender: ~/.config/eimer/sender.yaml)
    path: Option<PathBuf>,
    /// Overwrite an existing file
    #[arg(long)]
    force: bool,
}

pub fn run(args: DocArgs) -> Result<()> {
    match args.command {
        DocCommand::Render(args) => {
            let output = render::render(&RenderOptions {
                data: &args.data,
                output: args.output.as_deref(),
                sender: args.sender.as_deref(),
                watch: args.watch,
                png: args.png,
            })
            .context("Failed to render the document")?;
            if !args.watch {
                ui::print_success(&output.display().to_string());
            }
            Ok(())
        }
        DocCommand::New(args) => new(args).context("Failed to create the document"),
    }
}

fn new(args: NewArgs) -> Result<()> {
    let target = match (&args.path, args.kind.as_str()) {
        (Some(path), _) => path.clone(),
        (None, "sender") => render::default_sender()?,
        (None, kind) => PathBuf::from(format!("{kind}.yaml")),
    };
    if target.exists() && !args.force {
        return Err(anyhow!("{} already exists", target.display()))
            .hint("Pass --force to overwrite");
    }
    write(
        &target,
        assets::file(&format!("examples/{}.yaml", args.kind))?,
    )?;
    let next = if args.kind == "sender" {
        "Fill in your business details; every document uses them.".to_string()
    } else {
        format!("Edit it, then `eimer doc render {}`.", target.display())
    };
    ui::print_success(&format!("Wrote {}", config::with_home_path(&target)));
    ui::print_info(&next, None);
    Ok(())
}

fn write(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create {}", parent.display()))?;
    }
    std::fs::write(path, bytes).with_context(|| format!("Failed to write {}", path.display()))
}
