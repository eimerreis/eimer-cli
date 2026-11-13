//! `eimer product init`: writes `.eimer/product.json` from the repo's changelogs.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result, anyhow};
use clap::Args;

use super::config::{self, CONFIG_PATH, ProductConfig, ProductSource, default_internal_types};
use super::template::DEFAULT_TEMPLATE;
use crate::core::error::HintExt;
use crate::core::process::run_text;
use crate::core::{prompt, ui};

const TEMPLATE_PATH: &str = ".eimer/product-update.prompt.md";

#[derive(Args)]
pub struct InitArgs {
    /// Folder for update posts, relative to the repo root
    #[arg(long)]
    updates_dir: Option<String>,
    /// Folder for post media, relative to the repo root
    #[arg(long)]
    media_dir: Option<String>,
    /// URL prefix the media folder is served under
    #[arg(long, default_value = "/updates")]
    media_url: String,
    /// Languages, primary first
    #[arg(long, value_delimiter = ',', default_value = "en")]
    languages: Vec<String>,
    /// Overwrite an existing config
    #[arg(long)]
    force: bool,
}

pub fn run(args: InitArgs) -> Result<()> {
    let root = run_text(["git", "rev-parse", "--show-toplevel"])
        .context("Failed to find the git repository")
        .hint("Run `eimer product init` inside the repo.")?;
    let root = Path::new(root.trim());
    if root.join(CONFIG_PATH).exists() && !args.force {
        return Err(anyhow!("{CONFIG_PATH} already exists")).hint("Pass --force to overwrite it.");
    }

    let products = detect_products(root)?;
    if products.is_empty() {
        return Err(anyhow!("No CHANGELOG.md next to a package.json found"))
            .hint("Release with Changesets first, or write .eimer/product.json by hand.");
    }

    let interactive = prompt::is_interactive();
    let updates_dir = match args.updates_dir {
        Some(dir) => dir,
        None if interactive => prompt::text_or("Updates folder", "content/updates", |_| None)?,
        None => "content/updates".into(),
    };
    let media_dir = match args.media_dir {
        Some(dir) => dir,
        None if interactive => prompt::text_or("Media folder", "public/updates", |_| None)?,
        None => "public/updates".into(),
    };

    let config = ProductConfig {
        since: Some(chrono::Local::now().format("%Y-%m-%d").to_string()),
        products,
        platforms: BTreeMap::new(),
        internal_types: default_internal_types(),
        updates_dir,
        media_dir,
        media_url: args.media_url,
        languages: args.languages,
        capture: BTreeMap::new(),
        prompt_template: Some(TEMPLATE_PATH.into()),
        ignore_file: None,
    };
    config::save(root, &config)?;
    let template = root.join(TEMPLATE_PATH);
    if !template.exists() {
        std::fs::write(&template, DEFAULT_TEMPLATE)
            .with_context(|| format!("Failed to write {TEMPLATE_PATH}"))?;
    }

    ui::print_success(&format!(
        "Wrote {CONFIG_PATH} with {} products",
        config.products.len()
    ));
    ui::print_info(
        "Add `platforms` (tag → path globs) and `capture` commands, and set `include: false` on products users never see.",
        Some(&format!(
            "Prompt template: {TEMPLATE_PATH}. Next: eimer product status"
        )),
    );
    Ok(())
}

/// Tracked `CHANGELOG.md` files with a `package.json` beside them, keyed by package name.
fn detect_products(root: &Path) -> Result<BTreeMap<String, ProductSource>> {
    let root_str = root.to_string_lossy();
    let files = run_text(["git", "-C", &root_str, "ls-files", "--", "*CHANGELOG.md"])
        .context("Failed to list changelogs")?;
    let ignored = changeset_ignored(root);
    let mut products = BTreeMap::new();
    for file in files.lines().filter(|f| f.ends_with("CHANGELOG.md")) {
        let dir = Path::new(file).parent().unwrap_or(Path::new(""));
        let Ok(raw) = std::fs::read_to_string(root.join(dir).join("package.json")) else {
            continue;
        };
        let Some(name) = serde_json::from_str::<serde_json::Value>(&raw)
            .ok()
            .and_then(|v| v.get("name")?.as_str().map(String::from))
        else {
            continue;
        };
        if ignored.contains(&name) || dir.as_os_str().is_empty() {
            continue;
        }
        products.insert(
            name,
            ProductSource {
                changelog: file.to_string(),
                include: true,
            },
        );
    }
    Ok(products)
}

fn changeset_ignored(root: &Path) -> Vec<String> {
    std::fs::read_to_string(root.join(".changeset/config.json"))
        .ok()
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
        .and_then(|v| {
            v.get("ignore")?.as_array().map(|items| {
                items
                    .iter()
                    .filter_map(|i| i.as_str().map(String::from))
                    .collect()
            })
        })
        .unwrap_or_default()
}
