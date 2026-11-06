//! `release configure`: wizard for release defaults and Teams channels.

use std::collections::BTreeMap;

use anyhow::{Context, Result};
use clap::Args;

use super::api::{RunQuery, list_pipeline_stages, load_pipeline_runs, resolve_pipeline_by_name};
use super::client::AzureDevOpsClient;
use super::config::{
    ReleaseConfig, ReleaseSection, TeamsSection, config_path, load_config, mask_webhook_url,
    resolve_channels, save_config,
};
use super::prod_stage::{StageRecord, detect_prod_stage_name};
use super::util::text_with_fallback;
use crate::core::config::with_home_path;
use crate::core::error::HintExt;
use crate::core::{prompt, ui};

#[derive(Args)]
pub struct ConfigureArgs {
    /// Show current release config
    #[arg(long, short = 's')]
    show: bool,
}

pub fn run(args: ConfigureArgs) -> Result<()> {
    configure(args)
        .context("Failed to configure release CLI")
        .hint("Run `release configure --show` to inspect the current config.")
}

fn configure(args: ConfigureArgs) -> Result<()> {
    let display_path = with_home_path(&config_path()?);
    let current = load_config()?;
    if args.show {
        print_config_summary(&current, &display_path);
        return Ok(());
    }
    let next = prompt_for_config(&current)?;
    save_config(&next)?;
    ui::print_success(&format!("Saved release config to {display_path}."));
    Ok(())
}

fn no_validation(_: &str) -> Option<String> {
    None
}

fn prompt_for_config(current: &ReleaseConfig) -> Result<ReleaseConfig> {
    let default_pipeline = text_with_fallback(
        "Default release pipeline",
        &current.default_pipeline(),
        no_validation,
    )?;

    let current_branch = current.default_branch();
    let default_branch = text_with_fallback(
        "Release branch ref",
        if current_branch.is_empty() {
            "refs/heads/master"
        } else {
            &current_branch
        },
        no_validation,
    )?;

    let current_stage = current.prod_stage_name();
    let prod_stage_name = if default_pipeline.is_empty() {
        text_with_fallback(
            "Production stage name (optional)",
            &current_stage,
            no_validation,
        )?
    } else {
        let branch = if default_branch.is_empty() {
            "refs/heads/master"
        } else {
            &default_branch
        };
        let options = resolve_stage_options(&default_pipeline, branch)?;
        prompt_for_prod_stage_name(&options, &current_stage)?
    };

    let channels = prompt_for_channels(current)?;
    let mut named = BTreeMap::new();
    let mut primary = None;
    for (name, url) in channels {
        if name == "default" {
            primary = Some(url);
        } else {
            named.insert(name, url);
        }
    }

    let some = |v: String| Some(v).filter(|v| !v.is_empty());
    let release = ReleaseSection {
        default_pipeline: some(default_pipeline),
        prod_stage_name: some(prod_stage_name),
        default_branch: some(default_branch),
        extra: current
            .release
            .as_ref()
            .map(|r| r.extra.clone())
            .unwrap_or_default(),
    };
    let has_release = release.default_pipeline.is_some()
        || release.prod_stage_name.is_some()
        || release.default_branch.is_some();
    let teams = (primary.is_some() || !named.is_empty()).then(|| TeamsSection {
        webhook_url: primary,
        channels: (!named.is_empty()).then_some(named),
        extra: current
            .teams
            .as_ref()
            .map(|t| t.extra.clone())
            .unwrap_or_default(),
    });

    Ok(ReleaseConfig {
        teams,
        release: has_release.then_some(release),
        areas: current.areas.clone(),
        extra: current.extra.clone(),
    })
}

fn resolve_stage_options(pipeline_name: &str, branch: &str) -> Result<Vec<String>> {
    let client = ui::with_spinner("Loading Azure DevOps context", AzureDevOpsClient::create)?;
    let pipeline = ui::with_spinner("Resolving pipeline", || {
        resolve_pipeline_by_name(&client, pipeline_name)
    })?;
    let runs = ui::with_spinner("Loading recent pipeline runs", || {
        load_pipeline_runs(
            &client,
            RunQuery {
                pipeline_id: pipeline.id,
                top: Some(30),
                status: Some("completed"),
                result: None,
                branch: Some(branch),
            },
        )
    })?;
    for run in runs {
        let stages = list_pipeline_stages(&client, run.id)?;
        if !stages.is_empty() {
            return Ok(collect_stage_option_names(&stages));
        }
    }
    Ok(Vec::new())
}

/// Unique stage names, sorted, with the inferred prod stage first.
fn collect_stage_option_names(stages: &[StageRecord]) -> Vec<String> {
    let inferred = detect_prod_stage_name(stages);
    let mut names: Vec<String> = stages
        .iter()
        .map(|s| {
            if s.identifier.is_empty() {
                &s.name
            } else {
                &s.identifier
            }
        })
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .collect();
    let mut seen = std::collections::HashSet::new();
    names.retain(|n| seen.insert(n.clone()));
    names.sort_by_key(|n| n.to_lowercase());
    match inferred {
        Some(first) => std::iter::once(first.clone())
            .chain(names.into_iter().filter(|n| *n != first))
            .collect(),
        None => names,
    }
}

fn prompt_for_prod_stage_name(options: &[String], current: &str) -> Result<String> {
    if options.is_empty() {
        return text_with_fallback("Production stage name (optional)", current, no_validation);
    }

    ui::print_info(
        &format!("Detected pipeline stages: {}", options.join(", ")),
        None,
    );
    let mut labels = options.to_vec();
    labels.push("Custom stage name (Type an explicit stage identifier or name)".into());
    labels.push("No prod stage override (Use auto-detection only)".into());
    let default = options.iter().position(|o| o == current).unwrap_or(0);
    let index = prompt::select("Production stage", &labels, default)?;

    if index == options.len() + 1 {
        return Ok(String::new());
    }
    if index == options.len() {
        return text_with_fallback("Production stage name", current, no_validation);
    }
    Ok(options[index].clone())
}

fn prompt_for_channels(current: &ReleaseConfig) -> Result<Vec<(String, String)>> {
    let existing = resolve_channels(current);
    let mut channels = existing.clone();
    if !existing.is_empty() {
        let choice = prompt::select(
            "Existing Teams channels found",
            &[
                "Keep and edit (Add or overwrite channels)".to_string(),
                "Start fresh (Discard existing channels in this run)".to_string(),
            ],
            0,
        )?;
        if choice == 1 {
            channels.clear();
        }
    }

    loop {
        let name = text_with_fallback(
            "Teams channel name (leave empty to finish)",
            "",
            no_validation,
        )?;
        if name.is_empty() {
            return Ok(channels);
        }
        let existing_url = channels
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, u)| u.clone())
            .unwrap_or_default();
        let url = text_with_fallback(&format!("Webhook URL for '{name}'"), &existing_url, |v| {
            (v.trim().is_empty() && existing_url.is_empty())
                .then(|| "Webhook URL is required".to_string())
        })?;
        match channels.iter_mut().find(|(n, _)| *n == name) {
            Some(entry) => entry.1 = url,
            None => channels.push((name, url)),
        }
    }
}

fn print_config_summary(config: &ReleaseConfig, display_path: &str) {
    let mut channels = resolve_channels(config);
    channels.sort_by_key(|(name, _)| name.to_lowercase());
    let or = |value: String, fallback: &str| {
        if value.is_empty() {
            fallback.to_string()
        } else {
            value
        }
    };

    println!("Config path: {display_path}");
    println!(
        "Default pipeline: {}",
        or(config.default_pipeline(), "(not set)")
    );
    println!(
        "Release branch ref: {}",
        or(
            config.default_branch(),
            "refs/heads/master (built-in default)"
        )
    );
    println!(
        "Prod stage override: {}",
        or(config.prod_stage_name(), "(auto-detect)")
    );
    if channels.is_empty() {
        println!("Teams channels: (none)");
        return;
    }
    println!("Teams channels:");
    for (name, url) in channels {
        println!("- {name}: {}", mask_webhook_url(&url));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stage(identifier: &str, name: &str, ok: bool) -> StageRecord {
        StageRecord {
            identifier: identifier.into(),
            name: name.into(),
            result: if ok { "succeeded" } else { "failed" }.into(),
            state: "completed".into(),
        }
    }

    #[test]
    fn stage_options_put_inferred_first() {
        let stages = vec![
            stage("build", "Build", true),
            stage("deploy_prod", "Deploy Prod", true),
            stage("Approve", "", true),
            stage("build", "Build again", true),
            stage("", "", true),
        ];
        assert_eq!(
            collect_stage_option_names(&stages),
            vec!["deploy_prod", "Approve", "build"]
        );
    }
}
