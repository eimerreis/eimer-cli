//! `task context show|list|use`.

use anyhow::{Context, Result, bail};
use clap::{Args, Subcommand};
use serde_json::json;

use super::work_items::TaskEnv;
use crate::core::config::{load_config, update_config, with_home_path};
use crate::core::context::{CONTEXT_ENV_VAR, resolve_current, write_rc_file};
use crate::core::ui;

#[derive(Args)]
pub struct ContextArgs {
    #[command(subcommand)]
    command: ContextCommand,
}

#[derive(Subcommand)]
enum ContextCommand {
    /// Show the active context and where it was resolved from
    Show {
        /// Print machine-readable JSON
        #[arg(long, short = 'j')]
        json: bool,
    },
    /// List contexts defined in config.json
    List {
        /// Print machine-readable JSON
        #[arg(long, short = 'j')]
        json: bool,
    },
    /// Activate a context for the current directory (writes .eimerrc)
    Use {
        /// Context name from config.json
        name: String,
        /// Set defaultContext in config.json instead of writing .eimerrc
        #[arg(long, short = 'g')]
        global: bool,
    },
}

pub fn run(ctx: Option<&str>, args: ContextArgs) -> Result<()> {
    match args.command {
        ContextCommand::Show { json } => show(ctx, json).context("Failed to show context"),
        ContextCommand::List { json } => list(ctx, json).context("Failed to list contexts"),
        ContextCommand::Use { name, global } => {
            use_context(&name, global).context("Failed to switch context")
        }
    }
}

fn show(ctx: Option<&str>, json: bool) -> Result<()> {
    let env = TaskEnv::init(ctx, json)?;
    let team = env.default_team();
    let area_path = env.default_area_path();

    if json {
        let value = match &env.scope.context {
            Some(c) => {
                let mut v = serde_json::to_value(c)?;
                v["team"] = json!(team);
                v["areaPath"] = json!(area_path);
                v
            }
            None => json!({ "name": null, "source": null, "team": team, "areaPath": area_path }),
        };
        return ui::print_json(&value);
    }

    let Some(c) = &env.scope.context else {
        ui::print_info(
            "No context active - org/project are detected from the git remote.",
            Some(
                "Define contexts under `contexts` in ~/.config/eimer/config.json, then run `task context use <name>`.",
            ),
        );
        return Ok(());
    };

    for (label, value) in [
        ("Context", c.name.clone()),
        ("Org", c.org_url.clone()),
        ("Project", c.project.clone()),
        ("Team", team),
        ("Area path", area_path),
        ("Source", c.describe_source()),
    ] {
        println!(
            "{} {value}",
            ui::bold(&format!("{:<10}", format!("{label}:")))
        );
    }
    Ok(())
}

fn list(ctx: Option<&str>, json: bool) -> Result<()> {
    let config = load_config()?;
    let active = resolve_current(ctx, &config).ok().flatten().map(|c| c.name);

    if json {
        let items: Vec<_> = config
            .contexts
            .iter()
            .map(|(name, def)| {
                let mut v = serde_json::to_value(def).unwrap_or_default();
                v["name"] = json!(name);
                v["active"] = json!(active.as_deref() == Some(name.as_str()));
                v
            })
            .collect();
        return ui::print_json(&items);
    }

    if config.contexts.is_empty() {
        ui::print_info(
            "No contexts defined.",
            Some(
                r#"Add them to ~/.config/eimer/config.json, e.g. "contexts": { "tapio": { "org": "tapioone", "project": "tapio" } }"#,
            ),
        );
        return Ok(());
    }

    let rows = config.contexts.iter().map(|(name, def)| {
        vec![
            if active.as_deref() == Some(name.as_str()) {
                ui::symbols::ok()
            } else {
                String::new()
            },
            name.clone(),
            def.org.clone(),
            def.project.clone(),
            def.team.clone().unwrap_or_else(|| "-".into()),
            def.area_path.clone().unwrap_or_else(|| "-".into()),
        ]
    });
    println!(
        "{}",
        ui::render_table(["", "Context", "Org", "Project", "Team", "Area path"], rows)
    );
    Ok(())
}

fn use_context(name: &str, global: bool) -> Result<()> {
    let config = load_config()?;
    if !config.contexts.contains_key(name) {
        let known: Vec<&str> = config.contexts.keys().map(String::as_str).collect();
        let hint = if known.is_empty() {
            "No contexts are defined in config.json.".to_string()
        } else {
            format!("Known contexts: {}.", known.join(", "))
        };
        bail!("Unknown context '{name}'. {hint}");
    }

    if global {
        update_config(|c| c.default_context = Some(name.to_string()))?;
        ui::print_success(&format!("Set default context to '{name}'."));
    } else {
        let path = write_rc_file(&std::env::current_dir()?, name)?;
        ui::print_success(&format!(
            "Using context '{name}' in {}.",
            with_home_path(&path)
        ));
    }

    if let Some(env) = std::env::var(CONTEXT_ENV_VAR)
        .ok()
        .filter(|v| !v.trim().is_empty())
    {
        ui::print_warning(
            &format!(
                "{CONTEXT_ENV_VAR} is set to '{}' and takes precedence.",
                env.trim()
            ),
            None,
        );
    }
    Ok(())
}
