//! codeunity time REST client. Contract: https://backend.timeunity.de/openapi/v1.json

use anyhow::{Context, Result, anyhow, bail};
use reqwest::Url;
use reqwest::blocking::Client;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

const DEFAULT_BASE_URL: &str = "https://backend.timeunity.de/api";

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub id: u64,
    pub name: String,
    #[serde(default)]
    pub project_number: Option<String>,
    #[serde(default)]
    pub is_completed: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimeRecord {
    pub id: u64,
    pub description: String,
    /// hours * 100
    pub duration: i64,
    /// yyyy-mm-dd (the API may append a time part)
    #[serde(default)]
    pub date: String,
    #[serde(default)]
    pub invoiced: bool,
}

/// Budget progress from the dashboard overview, as the API reports it.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectProgress {
    pub id: u64,
    #[serde(default)]
    pub progress: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TimeRecordInput {
    pub project_id: u64,
    pub date: String,
    pub description: String,
    pub duration: i64,
    pub on_site: bool,
}

pub struct TimeClient {
    http: Client,
    token: String,
    base_url: String,
}

impl TimeClient {
    pub fn create() -> Result<Self> {
        Ok(Self {
            http: crate::core::http::client(),
            token: super::auth::access_token()?,
            base_url: std::env::var("EIMER_TIME_API_URL")
                .unwrap_or_else(|_| DEFAULT_BASE_URL.into()),
        })
    }

    fn send(&self, method: reqwest::Method, url: Url, body: Option<String>) -> Result<String> {
        let path = url.path().to_string();
        let mut request = self
            .http
            .request(method, url)
            .bearer_auth(&self.token)
            .header("Content-Type", "application/json");
        if let Some(body) = body {
            request = request.body(body);
        }
        let response = request
            .send()
            .with_context(|| format!("Failed to reach time API ({path})"))?;
        let status = response.status();
        let text = response.text().unwrap_or_default();
        if !status.is_success() {
            bail!("time API {path} failed: {status} {text}");
        }
        Ok(text)
    }

    fn url(&self, path: &str, query: &[(&str, &str)]) -> Result<Url> {
        let mut url = Url::parse(&format!("{}/{path}", self.base_url))
            .context("Failed to build time API URL")?;
        if !query.is_empty() {
            url.query_pairs_mut().extend_pairs(query);
        }
        Ok(url)
    }

    fn get<T: DeserializeOwned>(&self, path: &str, query: &[(&str, &str)]) -> Result<T> {
        let text = self.send(reqwest::Method::GET, self.url(path, query)?, None)?;
        serde_json::from_str(&text).with_context(|| format!("Failed to parse time API {path}"))
    }

    fn post(&self, path: &str, body: &impl Serialize) -> Result<String> {
        let body = serde_json::to_string(body)?;
        self.send(reqwest::Method::POST, self.url(path, &[])?, Some(body))
    }

    pub fn projects(&self) -> Result<Vec<Project>> {
        #[derive(Deserialize)]
        struct Response {
            projects: Vec<Project>,
        }
        Ok(self.get::<Response>("projects.get", &[])?.projects)
    }

    /// Ids of projects that have time records not yet invoiced.
    pub fn unbilled_project_ids(&self) -> Result<Vec<u64>> {
        #[derive(Deserialize)]
        struct Id {
            id: u64,
        }
        #[derive(Deserialize)]
        struct Response {
            projects: Vec<Id>,
        }
        Ok(self
            .get::<Response>("report.get-projects-with-unbilled-timerecords", &[])?
            .projects
            .into_iter()
            .map(|p| p.id)
            .collect())
    }

    pub fn project_overview(&self) -> Result<Vec<ProjectProgress>> {
        #[derive(Deserialize)]
        struct Response {
            projects: Vec<ProjectProgress>,
        }
        Ok(self
            .get::<Response>("dashboard.get-project-overview", &[])?
            .projects)
    }

    /// Every record of a project, regardless of date.
    pub fn all_time_records(&self, project_id: u64) -> Result<Vec<TimeRecord>> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Response {
            time_records: Vec<TimeRecord>,
        }
        let project = project_id.to_string();
        Ok(self
            .get::<Response>("timerecords.get", &[("projectId", project.as_str())])?
            .time_records)
    }

    pub fn time_records(&self, project_id: u64, from: &str, to: &str) -> Result<Vec<TimeRecord>> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Response {
            time_records: Vec<TimeRecord>,
        }
        let project = project_id.to_string();
        let query = [
            ("projectId", project.as_str()),
            ("fromDate", from),
            ("toDate", to),
        ];
        Ok(self
            .get::<Response>("timerecords.get", &query)?
            .time_records)
    }

    /// Creates a record and returns its id. The endpoint answers 204 without a body,
    /// so the record is looked up by date, description and duration (newest wins).
    pub fn create_record(&self, input: &TimeRecordInput) -> Result<u64> {
        self.post("timerecords.create-multiple", &[input])?;
        self.time_records(input.project_id, &input.date, &input.date)?
            .into_iter()
            .filter(|r| r.description == input.description && r.duration == input.duration)
            .map(|r| r.id)
            .max()
            .ok_or_else(|| {
                anyhow!("Record was created but its id could not be resolved. Check time before booking again.")
            })
    }
}
