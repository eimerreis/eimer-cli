//! `eimer configure`: manage shared defaults in ~/.config/eimer/config.json.
//! Contexts are managed via `task context` / editing the file and are always preserved.

use anyhow::{Context, Result};
use clap::{Args, ValueEnum};

use crate::core::config::{
    EimerConfig, ReleaseDefaults, TaskConfig, TeamsConfig, config_path, delete_config, load_config,
    save_config, with_home_path,
};
use crate::core::error::HintExt;
use crate::core::{prompt, ui};

#[derive(Args)]
pub struct ConfigureArgs {
    /// Show current config
    #[arg(long, short = 's')]
    show: bool,
    /// Delete config file
    #[arg(long, short = 'r')]
    reset: bool,
    /// Config key to set
    #[arg(long, short = 'k')]
    key: Option<ConfigKey>,
    /// Config value to set; empty string clears
    #[arg(long, short = 'v', requires = "key")]
    value: Option<String>,
}

#[derive(Clone, Copy, ValueEnum)]
enum ConfigKey {
    #[value(name = "teams.webhookUrl")]
    TeamsWebhookUrl,
    #[value(name = "task.defaultTeam")]
    TaskDefaultTeam,
    #[value(name = "task.defaultAreaPath")]
    TaskDefaultAreaPath,
    #[value(name = "release.defaultPipeline")]
    ReleaseDefaultPipeline,
    #[value(name = "defaultContext")]
    DefaultContext,
}

pub fn run(args: ConfigureArgs) -> Result<()> {
    configure(args)
        .context("Failed to configure eimer")
        .hint("Try `eimer configure --show` to inspect the current config before changing values.")
}

fn configure(args: ConfigureArgs) -> Result<()> {
    let path = with_home_path(&config_path()?);

    if args.reset {
        delete_config()?;
        ui::print_success(&format!("Deleted config at {path}."));
        return Ok(());
    }

    if args.show {
        return ui::print_json(&load_config()?);
    }

    if let Some(key) = args.key {
        let mut config = load_config()?;
        set_value(&mut config, key, args.value.as_deref().unwrap_or("").trim());
        save_config(&prune(config))?;
        ui::print_success(&format!(
            "Updated {} in {path}.",
            key.to_possible_value()
                .map(|v| v.get_name().to_string())
                .unwrap_or_default()
        ));
        return Ok(());
    }

    let mut config = load_config()?;
    let ask = |message: &str, current: Option<&String>| -> Result<String> {
        prompt::text(message, current.map(String::as_str), |_| None)
    };
    let webhook = ask(
        "Teams webhook URL",
        config.teams.as_ref().and_then(|t| t.webhook_url.as_ref()),
    )?;
    let team = ask(
        "Default Azure DevOps team",
        config.task.as_ref().and_then(|t| t.default_team.as_ref()),
    )?;
    let area = ask(
        "Default Azure DevOps area path",
        config
            .task
            .as_ref()
            .and_then(|t| t.default_area_path.as_ref()),
    )?;
    let pipeline = ask(
        "Default release pipeline name",
        config
            .release
            .as_ref()
            .and_then(|r| r.default_pipeline.as_ref()),
    )?;

    set_value(&mut config, ConfigKey::TeamsWebhookUrl, &webhook);
    set_value(&mut config, ConfigKey::TaskDefaultTeam, &team);
    set_value(&mut config, ConfigKey::TaskDefaultAreaPath, &area);
    set_value(&mut config, ConfigKey::ReleaseDefaultPipeline, &pipeline);
    save_config(&prune(config))?;
    ui::print_success(&format!("Saved config to {path}."));
    Ok(())
}

fn opt(value: &str) -> Option<String> {
    (!value.is_empty()).then(|| value.to_string())
}

fn set_value(config: &mut EimerConfig, key: ConfigKey, value: &str) {
    match key {
        ConfigKey::TeamsWebhookUrl => {
            config
                .teams
                .get_or_insert_with(TeamsConfig::default)
                .webhook_url = opt(value)
        }
        ConfigKey::TaskDefaultTeam => {
            config
                .task
                .get_or_insert_with(TaskConfig::default)
                .default_team = opt(value)
        }
        ConfigKey::TaskDefaultAreaPath => {
            config
                .task
                .get_or_insert_with(TaskConfig::default)
                .default_area_path = opt(value)
        }
        ConfigKey::ReleaseDefaultPipeline => {
            config
                .release
                .get_or_insert_with(ReleaseDefaults::default)
                .default_pipeline = opt(value)
        }
        ConfigKey::DefaultContext => config.default_context = opt(value),
    }
}

/// Drops empty sections so the file stays tidy. Contexts and unknown keys are kept.
fn prune(mut config: EimerConfig) -> EimerConfig {
    if config
        .teams
        .as_ref()
        .is_some_and(|t| t.webhook_url.is_none())
    {
        config.teams = None;
    }
    if config
        .task
        .as_ref()
        .is_some_and(|t| t.default_team.is_none() && t.default_area_path.is_none())
    {
        config.task = None;
    }
    if config
        .release
        .as_ref()
        .is_some_and(|r| r.default_pipeline.is_none())
    {
        config.release = None;
    }
    config
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_and_prune_keep_contexts() {
        let mut config = crate::core::config::parse_config(
            r#"{"contexts":{"t":{"org":"o","project":"p"}},"task":{"defaultTeam":"A"}}"#,
        )
        .unwrap();
        set_value(&mut config, ConfigKey::TaskDefaultTeam, "");
        let config = prune(config);
        assert!(config.task.is_none());
        assert!(config.contexts.contains_key("t"));
    }
}
