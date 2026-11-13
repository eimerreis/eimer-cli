//! Repo-level `.eimer/product.json`: which changelogs feed update posts and where posts live.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};

use crate::core::error::HintExt;

pub const CONFIG_PATH: &str = ".eimer/product.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductConfig {
    /// Changes committed before this date (YYYY-MM-DD) are never offered.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub since: Option<String>,
    /// Changesets product name → its changelog.
    pub products: BTreeMap<String, ProductSource>,
    /// Platform tag → path globs (`!` excludes) matched against a change's files.
    #[serde(default)]
    pub platforms: BTreeMap<String, Vec<String>>,
    /// Conventional-commit types that start unchecked.
    #[serde(default = "default_internal_types")]
    pub internal_types: Vec<String>,
    /// Folder of update posts (`<date>-<slug>.<lang>.md`), relative to the repo root.
    pub updates_dir: String,
    /// Folder for post media, relative to the repo root; each post gets `<mediaDir>/<slug>/`.
    pub media_dir: String,
    /// URL prefix under which `mediaDir` is served, e.g. `/updates`.
    #[serde(default = "default_media_url")]
    pub media_url: String,
    /// First language is the primary one; the draft is written in it.
    #[serde(default = "default_languages")]
    pub languages: Vec<String>,
    /// Platform tag → command that captures screenshots (`{test}` = UI test to run).
    #[serde(default)]
    pub capture: BTreeMap<String, String>,
    /// Prompt template, relative to the repo root. Built-in default when unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_template: Option<String>,
    /// Changes dismissed as internal. Defaults to `<updatesDir>/.ignored.json`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ignore_file: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProductSource {
    pub changelog: String,
    /// `false`: changes only in this product start unchecked (e.g. a backend).
    #[serde(default = "yes")]
    pub include: bool,
}

pub fn default_internal_types() -> Vec<String> {
    ["build", "chore", "ci", "docs", "refactor", "style", "test"]
        .map(String::from)
        .to_vec()
}

fn default_media_url() -> String {
    "/updates".into()
}

fn default_languages() -> Vec<String> {
    vec!["en".into()]
}

fn yes() -> bool {
    true
}

/// A repo with a product config.
#[derive(Debug, Clone)]
pub struct Repo {
    pub root: PathBuf,
    pub config: ProductConfig,
}

impl Repo {
    pub fn path(&self, relative: &str) -> PathBuf {
        self.root.join(relative)
    }

    pub fn updates_dir(&self) -> PathBuf {
        self.path(&self.config.updates_dir)
    }

    pub fn ignore_file(&self) -> PathBuf {
        match &self.config.ignore_file {
            Some(file) => self.path(file),
            None => self.updates_dir().join(".ignored.json"),
        }
    }

    pub fn primary_language(&self) -> &str {
        &self.config.languages[0]
    }

    /// `path` relative to the repo root, with `/` separators.
    pub fn relative(&self, path: &Path) -> String {
        path.strip_prefix(&self.root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/")
    }
}

/// Nearest directory at or above `start` that contains the config.
pub fn find_root(start: &Path) -> Option<PathBuf> {
    start
        .ancestors()
        .find(|dir| dir.join(CONFIG_PATH).is_file())
        .map(Path::to_path_buf)
}

pub fn load() -> Result<Repo> {
    let cwd = std::env::current_dir().context("Failed to read the current directory")?;
    let root = find_root(&cwd)
        .ok_or_else(|| anyhow!("No {CONFIG_PATH} found in this directory or above"))
        .hint("Run `eimer product init` in the repo root.")?;
    load_at(&root)
}

pub fn load_at(root: &Path) -> Result<Repo> {
    let raw = std::fs::read_to_string(root.join(CONFIG_PATH))
        .with_context(|| format!("Failed to read {CONFIG_PATH}"))?;
    let config = parse(&raw)?;
    Ok(Repo {
        root: root.to_path_buf(),
        config,
    })
}

pub fn parse(raw: &str) -> Result<ProductConfig> {
    let config: ProductConfig =
        serde_json::from_str(raw).with_context(|| format!("Failed to parse {CONFIG_PATH}"))?;
    validate(&config)?;
    Ok(config)
}

fn validate(config: &ProductConfig) -> Result<()> {
    if config.products.is_empty() {
        bail!("{CONFIG_PATH}: `products` is empty");
    }
    if config.languages.is_empty() {
        bail!("{CONFIG_PATH}: `languages` is empty");
    }
    if let Some(since) = &config.since
        && !is_iso_date(since)
    {
        bail!("{CONFIG_PATH}: `since` must be YYYY-MM-DD, got '{since}'");
    }
    if !config.media_url.starts_with('/') {
        bail!("{CONFIG_PATH}: `mediaUrl` must start with '/'");
    }
    Ok(())
}

pub fn save(root: &Path, config: &ProductConfig) -> Result<()> {
    let path = root.join(CONFIG_PATH);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).context("Failed to create .eimer/")?;
    }
    let raw = serde_json::to_string_pretty(config)? + "\n";
    std::fs::write(&path, raw).with_context(|| format!("Failed to write {CONFIG_PATH}"))
}

pub fn is_iso_date(value: &str) -> bool {
    chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d").is_ok() && value.len() == 10
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINIMAL: &str = r#"{
        "products": { "app": { "changelog": "CHANGELOG.md" } },
        "updatesDir": "content/updates",
        "mediaDir": "public/updates"
    }"#;

    #[test]
    fn applies_defaults() {
        let config = parse(MINIMAL).unwrap();
        assert!(config.products["app"].include);
        assert_eq!(config.languages, ["en"]);
        assert_eq!(config.media_url, "/updates");
        assert!(config.internal_types.contains(&"ci".to_string()));
    }

    #[test]
    fn rejects_bad_since() {
        let raw = MINIMAL.replace("\"products\"", "\"since\": \"13.11.2026\", \"products\"");
        assert!(parse(&raw).is_err());
    }

    #[test]
    fn finds_root_above_cwd() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".eimer")).unwrap();
        std::fs::write(dir.path().join(CONFIG_PATH), MINIMAL).unwrap();
        let nested = dir.path().join("a/b");
        std::fs::create_dir_all(&nested).unwrap();
        assert_eq!(find_root(&nested).unwrap(), dir.path());
    }
}
