//! Azure DevOps REST calls used by release commands (port of `utils.ts`, `approvals.ts`
//! and the timeline part of `prod-stage.ts`).

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::client::{AzureContext, AzureDevOpsClient};
use super::prod_stage::{StageRecord, normalize_text};

#[derive(Debug, Clone, Deserialize)]
pub struct PipelineDefinition {
    pub id: u64,
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PipelineRun {
    pub id: u64,
    pub status: Option<String>,
    pub result: Option<String>,
    pub source_branch: Option<String>,
    pub source_version: Option<String>,
    pub definition: Option<RunDefinition>,
    pub repository: Option<RunRepository>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct RunDefinition {
    pub id: Option<u64>,
    pub name: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct RunRepository {
    pub id: Option<String>,
}

impl PipelineRun {
    pub fn source_version(&self) -> String {
        self.source_version
            .as_deref()
            .unwrap_or_default()
            .trim()
            .to_string()
    }
}

#[derive(Deserialize)]
struct ListResponse<T> {
    #[serde(default = "Vec::new")]
    value: Vec<T>,
}

/// A commit returned by the Git commits API.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct GitCommit {
    pub commit_id: Option<String>,
    pub comment: Option<String>,
}

pub fn build_run_url(context: &AzureContext, run_id: u64) -> String {
    format!("{}/_build/results?buildId={run_id}", context.base_url)
}

pub fn resolve_pipeline_by_name(
    client: &AzureDevOpsClient,
    name: &str,
) -> Result<PipelineDefinition> {
    let response: ListResponse<PipelineDefinition> = client.get_json(
        "/_apis/build/definitions",
        vec![("name", Some(name.into())), ("$top", Some("20".into()))],
    )?;
    match response
        .value
        .into_iter()
        .find(|d| d.name.as_deref().unwrap_or_default().to_lowercase() == name.to_lowercase())
    {
        Some(definition) => Ok(definition),
        None => bail!("No pipeline found with name '{name}'."),
    }
}

pub fn load_run_by_id(client: &AzureDevOpsClient, id: u64) -> Result<PipelineRun> {
    client.get_json(&format!("/_apis/build/builds/{id}"), vec![])
}

#[derive(Default)]
pub struct RunQuery<'a> {
    pub pipeline_id: u64,
    pub top: Option<u32>,
    pub status: Option<&'a str>,
    pub result: Option<&'a str>,
    pub branch: Option<&'a str>,
}

pub fn load_pipeline_runs(client: &AzureDevOpsClient, q: RunQuery) -> Result<Vec<PipelineRun>> {
    let response: ListResponse<PipelineRun> = client.get_json(
        "/_apis/build/builds",
        vec![
            ("definitions", Some(q.pipeline_id.to_string())),
            ("$top", Some(q.top.unwrap_or(100).to_string())),
            ("queryOrder", Some("queueTimeDescending".into())),
            ("statusFilter", q.status.map(String::from)),
            ("resultFilter", q.result.map(String::from)),
            ("branchName", q.branch.map(String::from)),
        ],
    )?;
    Ok(response.value)
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct TimelineRecord {
    #[serde(rename = "type")]
    record_type: Option<String>,
    identifier: Option<String>,
    name: Option<String>,
    result: Option<String>,
    state: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Timeline {
    records: Option<Vec<TimelineRecord>>,
}

/// Stage records of a build timeline.
pub fn list_pipeline_stages(client: &AzureDevOpsClient, run_id: u64) -> Result<Vec<StageRecord>> {
    let timeline: Timeline =
        client.get_json(&format!("/_apis/build/builds/{run_id}/timeline"), vec![])?;
    Ok(timeline
        .records
        .unwrap_or_default()
        .into_iter()
        .filter(|r| normalize_text(r.record_type.as_deref().unwrap_or_default()) == "stage")
        .map(|r| StageRecord {
            identifier: r.identifier.unwrap_or_default().trim().to_string(),
            name: r.name.unwrap_or_default().trim().to_string(),
            result: normalize_text(r.result.as_deref().unwrap_or_default()),
            state: normalize_text(r.state.as_deref().unwrap_or_default()),
        })
        .collect())
}

pub fn load_commits(
    client: &AzureDevOpsClient,
    repository_id: &str,
    query: Vec<(&str, Option<String>)>,
) -> Result<Vec<GitCommit>> {
    let response: ListResponse<GitCommit> = client.get_json(
        &format!("/_apis/git/repositories/{repository_id}/commits"),
        query,
    )?;
    Ok(response.value)
}

// ---------------------------------------------------------------------------
// Approvals. Kept as raw JSON so `--json` passes the API objects through unchanged.

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Approval(pub Value);

impl Approval {
    pub fn id(&self) -> String {
        match &self.0["id"] {
            Value::String(s) => s.clone(),
            Value::Null => String::new(),
            other => other.to_string(),
        }
    }
    pub fn pipeline_name(&self) -> Option<String> {
        self.0["pipeline"]["name"]
            .as_str()
            .filter(|s| !s.is_empty())
            .map(String::from)
    }
    pub fn run_id(&self) -> Option<u64> {
        self.0["pipeline"]["owner"]["id"].as_u64()
    }
    pub fn created_on(&self) -> Option<String> {
        self.0["createdOn"]
            .as_str()
            .filter(|s| !s.is_empty())
            .map(String::from)
    }
}

pub fn load_pending_approvals(client: &AzureDevOpsClient) -> Result<Vec<Approval>> {
    let response: ListResponse<Approval> = client.get_json(
        "/_apis/pipelines/approvals",
        vec![("state", Some("pending".into()))],
    )?;
    Ok(response.value)
}

pub fn approve_approvals(
    client: &AzureDevOpsClient,
    approval_ids: &[String],
    comment: &str,
) -> Result<Vec<Approval>> {
    let payload: Vec<Value> = approval_ids
        .iter()
        .map(|id| serde_json::json!({ "approvalId": id, "status": "approved", "comment": comment }))
        .collect();
    let response: ListResponse<Approval> =
        client.patch_json("/_apis/pipelines/approvals", &Value::Array(payload), vec![])?;
    Ok(response.value)
}
