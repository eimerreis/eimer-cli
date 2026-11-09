//! Release config at `~/.config/tapio-release/config.json`, shared with the TS `@tapio/release` CLI.
//! Same schema and field names; unknown keys are preserved on save.

use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::core::config::home_dir;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub teams: Option<TeamsSection>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub release: Option<ReleaseSection>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub areas: Option<BTreeMap<String, AreaConfig>>,
    /// Keys this version does not know about.
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TeamsSection {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub webhook_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channels: Option<BTreeMap<String, String>>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseSection {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_pipeline: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prod_stage_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_branch: Option<String>,
    /// Remembered prod stage per pipeline name (set automatically by `changelog --prod-stage-name`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prod_stages: Option<BTreeMap<String, String>>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AreaConfig {
    pub include_scopes: Vec<String>,
    pub exclude_scopes: Vec<String>,
    pub exclude_keywords: Vec<String>,
}

impl ReleaseConfig {
    pub fn default_pipeline(&self) -> String {
        self.release_field(|r| r.default_pipeline.as_deref())
    }
    pub fn prod_stage_name(&self) -> String {
        self.release_field(|r| r.prod_stage_name.as_deref())
    }
    pub fn default_branch(&self) -> String {
        self.release_field(|r| r.default_branch.as_deref())
    }

    /// Remembered prod stage for a pipeline (case-insensitive name match).
    pub fn prod_stage_for(&self, pipeline: &str) -> Option<String> {
        let wanted = pipeline.trim();
        self.release
            .as_ref()?
            .prod_stages
            .as_ref()?
            .iter()
            .find(|(name, _)| name.trim().eq_ignore_ascii_case(wanted))
            .map(|(_, stage)| stage.trim().to_string())
            .filter(|stage| !stage.is_empty())
    }

    /// Records `stage` for `pipeline`, replacing an entry that differs only in case.
    /// Returns false when that stage was already remembered.
    pub fn remember_prod_stage(&mut self, pipeline: &str, stage: &str) -> bool {
        let (pipeline, stage) = (pipeline.trim(), stage.trim());
        if pipeline.is_empty() || stage.is_empty() {
            return false;
        }
        if self.prod_stage_for(pipeline).as_deref() == Some(stage) {
            return false;
        }
        let stages = self
            .release
            .get_or_insert_with(ReleaseSection::default)
            .prod_stages
            .get_or_insert_with(BTreeMap::new);
        stages.retain(|name, _| !name.trim().eq_ignore_ascii_case(pipeline));
        stages.insert(pipeline.to_string(), stage.to_string());
        true
    }
    fn release_field(&self, f: impl Fn(&ReleaseSection) -> Option<&str>) -> String {
        self.release
            .as_ref()
            .and_then(f)
            .unwrap_or_default()
            .trim()
            .to_string()
    }
}

pub fn config_dir() -> Result<PathBuf> {
    Ok(home_dir()
        .context("Could not resolve HOME directory for release config.")?
        .join(".config")
        .join("tapio-release"))
}

pub fn config_path() -> Result<PathBuf> {
    Ok(config_dir()?.join("config.json"))
}

pub fn load_config() -> Result<ReleaseConfig> {
    let path = config_path()?;
    if !path.exists() {
        return Ok(ReleaseConfig::default());
    }
    let raw = std::fs::read_to_string(&path)
        .with_context(|| format!("Failed to read config at {}", path.display()))?;
    let value: serde_json::Value = serde_json::from_str(&raw)
        .with_context(|| format!("Failed to parse config at {}", path.display()))?;
    parse_config(value).with_context(|| format!("Invalid config at {}", path.display()))
}

pub fn parse_config(value: serde_json::Value) -> Result<ReleaseConfig> {
    let mut config: ReleaseConfig = serde_json::from_value(value)?;
    normalize(&mut config)?;
    Ok(config)
}

/// Mirrors the TS zod schema: all strings are trimmed and must be non-empty.
fn normalize(config: &mut ReleaseConfig) -> Result<()> {
    fn required(field: &str, value: &mut String) -> Result<()> {
        *value = value.trim().to_string();
        if value.is_empty() {
            bail!("'{field}' must be a non-empty string");
        }
        Ok(())
    }
    fn optional(field: &str, value: &mut Option<String>) -> Result<()> {
        match value {
            Some(v) => required(field, v),
            None => Ok(()),
        }
    }
    fn map_keys<V>(field: &str, map: BTreeMap<String, V>) -> Result<BTreeMap<String, V>> {
        let mut out = BTreeMap::new();
        for (key, value) in map {
            let mut key = key;
            required(&format!("{field} key"), &mut key)?;
            out.insert(key, value);
        }
        Ok(out)
    }

    if let Some(teams) = &mut config.teams {
        optional("teams.webhookUrl", &mut teams.webhook_url)?;
        if let Some(channels) = teams.channels.take() {
            let mut channels = map_keys("teams.channels", channels)?;
            for (name, url) in &mut channels {
                required(&format!("teams.channels.{name}"), url)?;
            }
            teams.channels = Some(channels);
        }
    }
    if let Some(release) = &mut config.release {
        optional("release.defaultPipeline", &mut release.default_pipeline)?;
        optional("release.prodStageName", &mut release.prod_stage_name)?;
        optional("release.defaultBranch", &mut release.default_branch)?;
        if let Some(stages) = release.prod_stages.take() {
            let mut stages = map_keys("release.prodStages", stages)?;
            for (pipeline, stage) in &mut stages {
                required(&format!("release.prodStages.{pipeline}"), stage)?;
            }
            release.prod_stages = Some(stages);
        }
    }
    if let Some(areas) = config.areas.take() {
        let mut areas = map_keys("areas", areas)?;
        for (name, area) in &mut areas {
            for (list, label) in [
                (&mut area.include_scopes, "includeScopes"),
                (&mut area.exclude_scopes, "excludeScopes"),
                (&mut area.exclude_keywords, "excludeKeywords"),
            ] {
                for item in list.iter_mut() {
                    required(&format!("areas.{name}.{label}[]"), item)?;
                }
            }
        }
        config.areas = Some(areas);
    }
    Ok(())
}

pub fn save_config(config: &ReleaseConfig) -> Result<()> {
    let mut config = config.clone();
    normalize(&mut config)?;
    let dir = config_dir()?;
    std::fs::create_dir_all(&dir)?;
    let path = config_path()?;
    std::fs::write(
        &path,
        format!("{}\n", serde_json::to_string_pretty(&config)?),
    )
    .with_context(|| format!("Failed to write {}", path.display()))
}

/// Named webhook channels. The legacy `teams.webhookUrl` becomes the `default` channel.
/// Ordered with `default` first, then configured channels.
pub fn resolve_channels(config: &ReleaseConfig) -> Vec<(String, String)> {
    let mut channels: Vec<(String, String)> = Vec::new();
    let mut set = |name: String, url: String| {
        if let Some(entry) = channels.iter_mut().find(|(n, _)| *n == name) {
            entry.1 = url;
        } else {
            channels.push((name, url));
        }
    };

    let teams = config.teams.as_ref();
    let default_webhook = teams
        .and_then(|t| t.webhook_url.as_deref())
        .unwrap_or_default()
        .trim();
    if !default_webhook.is_empty() {
        set("default".into(), default_webhook.into());
    }
    if let Some(configured) = teams.and_then(|t| t.channels.as_ref()) {
        for (name, url) in configured {
            let (name, url) = (name.trim(), url.trim());
            if name.is_empty() || url.is_empty() {
                continue;
            }
            set(name.into(), url.into());
        }
    }
    channels
}

pub fn mask_webhook_url(value: &str) -> String {
    let trimmed = value.trim();
    let chars: Vec<char> = trimmed.chars().collect();
    if chars.len() <= 14 {
        return "***".into();
    }
    let head: String = chars[..10].iter().collect();
    let tail: String = chars[chars.len() - 4..].iter().collect();
    format!("{head}...{tail}")
}

#[cfg(test)]
mod tests {
    use super::super::areas::merge_area_configs;
    use super::*;
    use serde_json::json;

    #[test]
    fn remembers_prod_stage_per_pipeline() {
        let mut config = parse_config(json!({
            "release": {"prodStageName": "deploy_prod", "prodStages": {"tapio.B2C.UI": "deploy-tapiousers"}}
        }))
        .unwrap();
        // case-insensitive lookup, global override untouched
        assert_eq!(
            config.prod_stage_for("TAPIO.b2c.ui").as_deref(),
            Some("deploy-tapiousers")
        );
        assert_eq!(config.prod_stage_for("other"), None);
        assert_eq!(config.prod_stage_name(), "deploy_prod");

        // same stage again: nothing to save
        assert!(!config.remember_prod_stage("tapio.b2c.ui", "deploy-tapiousers"));
        // new stage replaces the entry that differs only in case
        assert!(config.remember_prod_stage("tapio.b2c.ui", "deploy_users"));
        let stages = config
            .release
            .as_ref()
            .unwrap()
            .prod_stages
            .as_ref()
            .unwrap();
        assert_eq!(stages.len(), 1);
        assert_eq!(stages["tapio.b2c.ui"], "deploy_users");
        // new pipeline is added
        assert!(config.remember_prod_stage("App", "release"));
        assert!(!config.remember_prod_stage(" ", "x"));

        let saved = serde_json::to_value(&config).unwrap();
        assert_eq!(saved["release"]["prodStages"]["App"], "release");
        assert_eq!(saved["release"]["prodStageName"], "deploy_prod");
    }

    #[test]
    fn rejects_empty_remembered_stage() {
        assert!(parse_config(json!({"release": {"prodStages": {"app": " "}}})).is_err());
    }

    #[test]
    fn parses_and_preserves_unknown_keys() {
        let config = parse_config(json!({
            "teams": {"webhookUrl": " https://hook ", "channels": {"fe": "https://fe"}},
            "release": {"defaultPipeline": "app", "prodStageName": "deploy_prod"},
            "areas": {"Mobile": {"includeScopes": ["ios"], "excludeScopes": [], "excludeKeywords": []}},
            "future": {"x": 1}
        }))
        .unwrap();
        assert_eq!(config.default_pipeline(), "app");
        assert_eq!(config.default_branch(), "");
        let back = serde_json::to_value(&config).unwrap();
        assert_eq!(back["future"]["x"], 1);
        assert_eq!(back["teams"]["webhookUrl"], "https://hook");
        assert_eq!(back["release"]["prodStageName"], "deploy_prod");
        assert!(back["release"].get("defaultBranch").is_none());

        let channels = resolve_channels(&config);
        assert_eq!(
            channels,
            vec![
                ("default".to_string(), "https://hook".to_string()),
                ("fe".to_string(), "https://fe".to_string())
            ]
        );

        let merged = merge_area_configs(config.areas.as_ref());
        assert_eq!(
            merged.keys().collect::<Vec<_>>(),
            vec!["backend", "frontend", "infra", "mobile"]
        );
    }

    #[test]
    fn rejects_empty_strings() {
        assert!(parse_config(json!({"release": {"defaultPipeline": "  "}})).is_err());
        assert!(parse_config(json!({"teams": {"channels": {"x": ""}}})).is_err());
    }

    #[test]
    fn masks_webhooks() {
        assert_eq!(mask_webhook_url("short"), "***");
        assert_eq!(
            mask_webhook_url("https://example.webhook.office.com/abcd"),
            "https://ex...abcd"
        );
    }
}
