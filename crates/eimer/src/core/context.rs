//! Named Azure DevOps contexts (see docs/features/007-task-contexts-and-az-style-commands.md).
//! Resolution order: `--context` flag > `EIMER_CONTEXT` > nearest `.eimerrc` > `defaultContext`.

use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, bail};
use serde::{Deserialize, Serialize};

use super::config::{ContextDefinition, EimerConfig, validate_context_name};

pub const RC_FILE_NAME: &str = ".eimerrc";
pub const CONTEXT_ENV_VAR: &str = "EIMER_CONTEXT";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ContextSource {
    Flag,
    Env,
    Rc,
    Global,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ContextSelection {
    pub name: String,
    pub source: ContextSource,
    pub rc_path: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedContext {
    pub name: String,
    pub org: String,
    pub project: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub team: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub area_path: Option<String>,
    pub source: ContextSource,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rc_path: Option<PathBuf>,
    /// Normalized organization URL, e.g. `https://dev.azure.com/tapioone`.
    pub org_url: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RcFile {
    pub path: PathBuf,
    pub context: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RcContent {
    context: String,
}

fn non_empty(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|v| !v.is_empty())
}

/// Pure precedence logic: flag > env > rc > global default.
pub fn select_context(
    flag: Option<&str>,
    env: Option<&str>,
    rc: Option<&RcFile>,
    default_context: Option<&str>,
) -> Option<ContextSelection> {
    if let Some(name) = non_empty(flag) {
        return Some(ContextSelection {
            name: name.into(),
            source: ContextSource::Flag,
            rc_path: None,
        });
    }
    if let Some(name) = non_empty(env) {
        return Some(ContextSelection {
            name: name.into(),
            source: ContextSource::Env,
            rc_path: None,
        });
    }
    if let Some(rc) = rc {
        return Some(ContextSelection {
            name: rc.context.clone(),
            source: ContextSource::Rc,
            rc_path: Some(rc.path.clone()),
        });
    }
    non_empty(default_context).map(|name| ContextSelection {
        name: name.into(),
        source: ContextSource::Global,
        rc_path: None,
    })
}

pub fn to_org_url(org: &str) -> String {
    let trimmed = org.trim().trim_end_matches('/');
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        trimmed.to_string()
    } else {
        format!("https://dev.azure.com/{trimmed}")
    }
}

pub fn describe_source(source: ContextSource, rc_path: Option<&Path>) -> String {
    match source {
        ContextSource::Flag => "--context flag".into(),
        ContextSource::Env => format!("{CONTEXT_ENV_VAR} env var"),
        ContextSource::Rc => rc_path
            .map(super::config::with_home_path)
            .unwrap_or_else(|| RC_FILE_NAME.into()),
        ContextSource::Global => "defaultContext in config.json".into(),
    }
}

impl ResolvedContext {
    pub fn describe_source(&self) -> String {
        describe_source(self.source, self.rc_path.as_deref())
    }
}

pub fn materialize(selection: ContextSelection, config: &EimerConfig) -> Result<ResolvedContext> {
    let Some(def) = config.contexts.get(&selection.name) else {
        let known: Vec<&str> = config.contexts.keys().map(String::as_str).collect();
        let hint = if known.is_empty() {
            "No contexts are defined in config.json.".to_string()
        } else {
            format!("Known contexts: {}.", known.join(", "))
        };
        bail!(
            "Unknown context '{}' (from {}). {hint}",
            selection.name,
            describe_source(selection.source, selection.rc_path.as_deref())
        );
    };
    let ContextDefinition {
        org,
        project,
        team,
        area_path,
    } = def.clone();
    Ok(ResolvedContext {
        name: selection.name,
        org_url: to_org_url(&org),
        org,
        project,
        team,
        area_path,
        source: selection.source,
        rc_path: selection.rc_path,
    })
}

fn read_rc_file(path: &Path) -> Result<RcFile> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read {}", path.display()))?;
    let content: RcContent = serde_json::from_str(&raw).map_err(|_| {
        anyhow::anyhow!(
            "Invalid {}: expected {{ \"context\": \"<name>\" }}",
            path.display()
        )
    })?;
    validate_context_name(&content.context)?;
    Ok(RcFile {
        path: path.to_path_buf(),
        context: content.context,
    })
}

/// Walks up from `start` to the filesystem root and returns the nearest `.eimerrc`.
pub fn find_rc_file(start: &Path) -> Result<Option<RcFile>> {
    for dir in start.ancestors() {
        let candidate = dir.join(RC_FILE_NAME);
        if candidate.is_file() {
            return read_rc_file(&candidate).map(Some);
        }
    }
    Ok(None)
}

pub fn write_rc_file(dir: &Path, context: &str) -> Result<PathBuf> {
    validate_context_name(context)?;
    let path = dir.join(RC_FILE_NAME);
    let content = serde_json::to_string_pretty(&RcContent {
        context: context.to_string(),
    })?;
    std::fs::write(&path, format!("{content}\n"))
        .with_context(|| format!("Failed to write {}", path.display()))?;
    Ok(path)
}

/// Resolves the active context. Returns `None` when nothing is configured (git-remote detection applies).
pub fn resolve_context(
    flag: Option<&str>,
    cwd: &Path,
    env: Option<&str>,
    config: &EimerConfig,
) -> Result<Option<ResolvedContext>> {
    // Only touch the filesystem when higher-precedence sources are absent.
    let rc = if non_empty(flag).is_some() || non_empty(env).is_some() {
        None
    } else {
        find_rc_file(cwd)?
    };
    select_context(flag, env, rc.as_ref(), config.default_context.as_deref())
        .map(|selection| materialize(selection, config))
        .transpose()
}

/// [`resolve_context`] using the process environment, cwd and the on-disk config.
pub fn resolve_current(
    flag: Option<&str>,
    config: &EimerConfig,
) -> Result<Option<ResolvedContext>> {
    let cwd = std::env::current_dir()?;
    let env = std::env::var(CONTEXT_ENV_VAR).ok();
    resolve_context(flag, &cwd, env.as_deref(), config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn config() -> EimerConfig {
        let mut contexts = BTreeMap::new();
        contexts.insert(
            "tapio".into(),
            ContextDefinition {
                org: "tapioone".into(),
                project: "tapio".into(),
                team: Some("Core".into()),
                area_path: None,
            },
        );
        contexts.insert(
            "homag".into(),
            ContextDefinition {
                org: "homag-group".into(),
                project: "onehomag".into(),
                team: None,
                area_path: None,
            },
        );
        EimerConfig {
            default_context: Some("tapio".into()),
            contexts,
            ..Default::default()
        }
    }

    fn rc() -> RcFile {
        RcFile {
            path: "/x/.eimerrc".into(),
            context: "rc-ctx".into(),
        }
    }

    #[test]
    fn precedence() {
        let rc = rc();
        assert_eq!(
            select_context(Some("f"), Some("e"), Some(&rc), Some("d"))
                .unwrap()
                .source,
            ContextSource::Flag
        );
        assert_eq!(
            select_context(None, Some("e"), Some(&rc), Some("d"))
                .unwrap()
                .source,
            ContextSource::Env
        );
        let from_rc = select_context(None, None, Some(&rc), Some("d")).unwrap();
        assert_eq!(
            (from_rc.name.as_str(), from_rc.source),
            ("rc-ctx", ContextSource::Rc)
        );
        assert_eq!(
            select_context(None, None, None, Some("d")).unwrap().source,
            ContextSource::Global
        );
        assert_eq!(select_context(Some("  "), Some(""), None, None), None);
    }

    #[test]
    fn org_urls() {
        assert_eq!(to_org_url("tapioone"), "https://dev.azure.com/tapioone");
        assert_eq!(
            to_org_url("https://dev.azure.com/homag-group/"),
            "https://dev.azure.com/homag-group"
        );
    }

    #[test]
    fn rc_discovery_and_resolution() {
        let root = tempfile::tempdir().unwrap();
        let nested = root.path().join("a/b/c");
        std::fs::create_dir_all(&nested).unwrap();
        write_rc_file(root.path(), "homag").unwrap();
        write_rc_file(&root.path().join("a"), "tapio").unwrap();

        let found = find_rc_file(&nested).unwrap().unwrap();
        assert_eq!(found.path, root.path().join("a/.eimerrc"));
        assert_eq!(found.context, "tapio");

        let cfg = config();
        let ctx = resolve_context(None, root.path(), None, &cfg)
            .unwrap()
            .unwrap();
        assert_eq!(
            (ctx.name.as_str(), ctx.source),
            ("homag", ContextSource::Rc)
        );
        assert_eq!(ctx.org_url, "https://dev.azure.com/homag-group");

        let ctx = resolve_context(None, root.path(), Some("tapio"), &cfg)
            .unwrap()
            .unwrap();
        assert_eq!(
            (ctx.source, ctx.team.as_deref()),
            (ContextSource::Env, Some("Core"))
        );

        let ctx = resolve_context(Some("homag"), root.path(), Some("tapio"), &cfg)
            .unwrap()
            .unwrap();
        assert_eq!(ctx.source, ContextSource::Flag);
    }

    #[test]
    fn malformed_rc_and_unknown_context() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join(".eimerrc"), r#"{ "ctx": 1 }"#).unwrap();
        assert!(
            find_rc_file(root.path())
                .unwrap_err()
                .to_string()
                .contains("Invalid")
        );

        let err = resolve_context(Some("nope"), root.path(), None, &config())
            .unwrap_err()
            .to_string();
        assert_eq!(
            err,
            "Unknown context 'nope' (from --context flag). Known contexts: homag, tapio."
        );
    }

    #[test]
    fn no_context_configured() {
        let root = tempfile::tempdir().unwrap();
        assert_eq!(
            resolve_context(None, root.path(), None, &EimerConfig::default()).unwrap(),
            None
        );
    }
}
