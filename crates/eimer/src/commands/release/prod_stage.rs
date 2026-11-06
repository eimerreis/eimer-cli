//! Production stage detection on build timelines (port of `prod-stage.ts`).

use std::collections::{BTreeSet, HashMap};

use anyhow::Result;

use super::api::list_pipeline_stages;
use super::client::AzureDevOpsClient;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StageRecord {
    pub identifier: String,
    pub name: String,
    /// Lowercased.
    pub result: String,
    /// Lowercased.
    pub state: String,
}

pub fn normalize_text(value: &str) -> String {
    value.trim().to_lowercase()
}

/// Caches "run has a successful prod stage" per run and stage override.
pub struct ProdStageChecker<'a> {
    client: &'a AzureDevOpsClient,
    prod_stage_name: String,
    cache: HashMap<u64, bool>,
}

impl<'a> ProdStageChecker<'a> {
    pub fn new(client: &'a AzureDevOpsClient, prod_stage_name: &str) -> Self {
        Self {
            client,
            prod_stage_name: normalize_text(prod_stage_name),
            cache: HashMap::new(),
        }
    }

    pub fn has_successful_prod_stage(&mut self, run_id: u64) -> Result<bool> {
        if let Some(cached) = self.cache.get(&run_id) {
            return Ok(*cached);
        }
        let stages = list_pipeline_stages(self.client, run_id)?;
        let has_prod = stages_have_successful_prod(&stages, &self.prod_stage_name);
        self.cache.insert(run_id, has_prod);
        Ok(has_prod)
    }
}

/// True when a completed+succeeded stage matches the configured name (if any) or the heuristics.
pub fn stages_have_successful_prod(stages: &[StageRecord], configured: &str) -> bool {
    let configured = normalize_text(configured);
    stages.iter().any(|stage| {
        if !is_successful_stage(stage) {
            return false;
        }
        if configured.is_empty() {
            looks_like_prod_stage(stage)
        } else {
            matches_configured_stage(stage, &configured)
        }
    })
}

pub fn detect_prod_stage_name(stages: &[StageRecord]) -> Option<String> {
    let candidate = stages
        .iter()
        .filter(|s| is_successful_stage(s))
        .find(|s| looks_like_prod_stage(s))
        .or_else(|| stages.iter().find(|s| looks_like_prod_stage(s)))?;
    [&candidate.identifier, &candidate.name]
        .into_iter()
        .find(|v| !v.is_empty())
        .cloned()
}

fn is_successful_stage(stage: &StageRecord) -> bool {
    stage.state == "completed" && stage.result == "succeeded"
}

fn matches_configured_stage(stage: &StageRecord, configured: &str) -> bool {
    let configured = canonical_stage_name(configured);
    configured == canonical_stage_name(&stage.identifier)
        || configured == canonical_stage_name(&stage.name)
}

pub fn looks_like_prod_stage(stage: &StageRecord) -> bool {
    let identifier = normalize_text(&stage.identifier);
    let name = normalize_text(&stage.name);
    if identifier.is_empty() && name.is_empty() {
        return false;
    }

    let parts: BTreeSet<&str> = split_stage_parts(&identifier)
        .chain(split_stage_parts(&name))
        .collect();
    let has = |p: &str| parts.contains(p);
    let has_deploy = has("deploy") || has("deployment");
    let has_prod = has("prod") || has("production");

    identifier == "deploy_prod" || has_prod || (has("live") && has_deploy) || has("release")
}

fn split_stage_parts(value: &str) -> impl Iterator<Item = &str> {
    value
        .split(|c: char| !(c.is_ascii_lowercase() || c.is_ascii_digit()))
        .filter(|p| !p.is_empty())
}

fn canonical_stage_name(value: &str) -> String {
    normalize_text(value)
        .chars()
        .filter(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        .collect()
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
    fn heuristics() {
        let yes = [
            stage("deploy_prod", "", true),
            stage("Deploy_Production", "", true),
            stage("", "Deploy Live", true),
            stage("release", "", true),
            stage("stage1", "PROD rollout", true),
        ];
        for s in &yes {
            assert!(looks_like_prod_stage(s), "{s:?}");
        }
        let no = [
            stage("", "", true),
            stage("build", "Build", true),
            stage("live", "Live smoke", true),
            stage("deploy_staging", "Deploy Staging", true),
            stage("preproduction", "", true),
        ];
        for s in &no {
            assert!(!looks_like_prod_stage(s), "{s:?}");
        }
    }

    #[test]
    fn successful_prod_detection() {
        let stages = vec![
            stage("build", "Build", true),
            stage("deploy_prod", "Deploy Prod", false),
            stage("deploy_customer_a", "Customer A", true),
        ];
        assert!(!stages_have_successful_prod(&stages, ""));
        assert!(stages_have_successful_prod(&stages, "Deploy-Customer-A"));
        assert!(stages_have_successful_prod(&stages, " customer a "));
        assert!(!stages_have_successful_prod(&stages, "deploy_prod"));

        // Configured name disables heuristics.
        let ok = vec![stage("deploy_prod", "", true)];
        assert!(stages_have_successful_prod(&ok, ""));
        assert!(!stages_have_successful_prod(&ok, "other"));
    }

    #[test]
    fn detects_stage_name() {
        let stages = vec![
            stage("build", "", true),
            stage("deploy_prod", "", false),
            stage("", "Release EU", true),
        ];
        assert_eq!(
            detect_prod_stage_name(&stages).as_deref(),
            Some("Release EU")
        );
        let failed_only = vec![stage("deploy_prod", "", false)];
        assert_eq!(
            detect_prod_stage_name(&failed_only).as_deref(),
            Some("deploy_prod")
        );
        assert_eq!(detect_prod_stage_name(&[stage("build", "", true)]), None);
    }
}
