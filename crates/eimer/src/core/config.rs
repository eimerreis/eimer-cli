//! Shared eimer config at `~/.config/eimer/config.json`.
//! Unknown keys are preserved on save so newer/older versions don't clobber each other.

use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EimerConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_context: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub contexts: BTreeMap<String, ContextDefinition>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub teams: Option<TeamsConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task: Option<TaskConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub release: Option<ReleaseDefaults>,
    /// Keys this version does not know about.
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ContextDefinition {
    pub org: String,
    pub project: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub team: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub area_path: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TeamsConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub webhook_url: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TaskConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_team: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_area_path: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseDefaults {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_pipeline: Option<String>,
}

pub fn home_dir() -> Result<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .ok_or_else(|| anyhow!("Could not resolve HOME directory"))
}

pub fn config_dir() -> Result<PathBuf> {
    Ok(home_dir()?.join(".config").join("eimer"))
}

pub fn config_path() -> Result<PathBuf> {
    Ok(config_dir()?.join("config.json"))
}

/// Replaces the home directory prefix with `~` for display.
pub fn with_home_path(path: &std::path::Path) -> String {
    let display = path.display().to_string();
    match home_dir() {
        Ok(home) => {
            let home = home.display().to_string();
            match display.strip_prefix(&home) {
                Some(rest) if !home.is_empty() => format!("~{rest}"),
                _ => display,
            }
        }
        Err(_) => display,
    }
}

pub fn load_config() -> Result<EimerConfig> {
    let path = config_path()?;
    if !path.exists() {
        return Ok(EimerConfig::default());
    }
    let raw = std::fs::read_to_string(&path)
        .with_context(|| format!("Failed to read {}", path.display()))?;
    parse_config(&raw).with_context(|| format!("Invalid config at {}", path.display()))
}

pub fn parse_config(raw: &str) -> Result<EimerConfig> {
    let config: EimerConfig = serde_json::from_str(raw)?;
    validate(&config)?;
    Ok(config)
}

fn validate(config: &EimerConfig) -> Result<()> {
    if let Some(name) = &config.default_context {
        validate_context_name(name)?;
    }
    for (name, def) in &config.contexts {
        validate_context_name(name)?;
        if def.org.trim().is_empty() || def.project.trim().is_empty() {
            bail!("Context '{name}' needs non-empty 'org' and 'project'");
        }
    }
    Ok(())
}

pub fn validate_context_name(name: &str) -> Result<()> {
    let mut chars = name.chars();
    let valid = matches!(chars.next(), Some(c) if c.is_ascii_alphanumeric())
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if !valid {
        bail!("Invalid context name '{name}': only letters, digits, '-' and '_' are allowed");
    }
    Ok(())
}

pub fn save_config(config: &EimerConfig) -> Result<()> {
    validate(config)?;
    let dir = config_dir()?;
    std::fs::create_dir_all(&dir)?;
    let path = config_path()?;
    std::fs::write(
        &path,
        format!("{}\n", serde_json::to_string_pretty(config)?),
    )
    .with_context(|| format!("Failed to write {}", path.display()))
}

pub fn update_config(update: impl FnOnce(&mut EimerConfig)) -> Result<EimerConfig> {
    let mut config = load_config()?;
    update(&mut config);
    save_config(&config)?;
    Ok(config)
}

pub fn delete_config() -> Result<()> {
    let path = config_path()?;
    if path.exists() {
        std::fs::remove_file(path)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_preserves_unknown_keys() {
        let raw = r#"{"defaultContext":"tapio","contexts":{"tapio":{"org":"tapioone","project":"tapio","areaPath":"a\\b"}},"future":{"x":1}}"#;
        let config = parse_config(raw).unwrap();
        assert_eq!(config.contexts["tapio"].area_path.as_deref(), Some("a\\b"));
        let back = serde_json::to_value(&config).unwrap();
        assert_eq!(back["future"]["x"], 1);
        assert_eq!(back["contexts"]["tapio"]["areaPath"], "a\\b");
    }

    #[test]
    fn rejects_bad_context_names_and_empty_org() {
        assert!(parse_config(r#"{"defaultContext":"bad name"}"#).is_err());
        assert!(parse_config(r#"{"contexts":{"x":{"org":"","project":"p"}}}"#).is_err());
    }
}
