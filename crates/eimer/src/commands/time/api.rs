//! codeunity time REST client (mirrors the Raycast extension's `timeClient.ts`).

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
    pub date: String,
    pub description: String,
    /// hours * 100
    pub duration: i64,
    #[serde(default)]
    pub invoiced: bool,
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

    /// Creates a record and returns its id. The response shape is undocumented, so the
    /// id is read from it when present, else looked up by date, description and duration.
    pub fn create_record(&self, input: &TimeRecordInput) -> Result<u64> {
        let text = self.post("timerecords.create-multiple", &[input])?;
        if let Some(id) = id_from_response(&text) {
            return Ok(id);
        }
        self.time_records(input.project_id, &input.date, &input.date)?
            .into_iter()
            .filter(|r| r.description == input.description && r.duration == input.duration)
            .map(|r| r.id)
            .max()
            .ok_or_else(|| {
                anyhow!("Record was created but its id could not be resolved. Check time before booking again.")
            })
    }

    pub fn update_record(&self, id: u64, input: &TimeRecordInput) -> Result<()> {
        #[derive(Serialize)]
        struct Update<'a> {
            id: u64,
            #[serde(flatten)]
            input: &'a TimeRecordInput,
        }
        self.post("timerecords.update-multiple", &[Update { id, input }])?;
        Ok(())
    }

    /// Looks the record up in the recent past (the API only lists by date range).
    pub fn find_record(&self, project_id: u64, id: u64) -> Result<Option<TimeRecord>> {
        let today = chrono::Local::now().date_naive();
        let from = today - chrono::Days::new(120);
        let records = self.time_records(
            project_id,
            &from.format("%Y-%m-%d").to_string(),
            &today.format("%Y-%m-%d").to_string(),
        )?;
        Ok(records.into_iter().find(|r| r.id == id))
    }
}

fn id_from_response(text: &str) -> Option<u64> {
    let data: serde_json::Value = serde_json::from_str(text).ok()?;
    let candidate = data
        .get(0)
        .or_else(|| data.get("timeRecords").and_then(|v| v.get(0)))
        .or_else(|| data.get("ids").and_then(|v| v.get(0)))
        .unwrap_or(&data);
    candidate
        .as_u64()
        .or_else(|| candidate.get("id").and_then(|id| id.as_u64()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_id_from_known_response_shapes() {
        assert_eq!(id_from_response("[12]"), Some(12));
        assert_eq!(id_from_response(r#"[{"id":12}]"#), Some(12));
        assert_eq!(id_from_response(r#"{"timeRecords":[{"id":12}]}"#), Some(12));
        assert_eq!(id_from_response(r#"{"ids":[12]}"#), Some(12));
        assert_eq!(id_from_response(r#"{"id":12}"#), Some(12));
        assert_eq!(id_from_response(""), None);
        assert_eq!(id_from_response(r#"{"ok":true}"#), None);
    }

    #[test]
    fn update_body_is_flat() {
        let input = TimeRecordInput {
            project_id: 1,
            date: "2026-11-06".into(),
            description: "d".into(),
            duration: 150,
            on_site: false,
        };
        #[derive(Serialize)]
        struct Update<'a> {
            id: u64,
            #[serde(flatten)]
            input: &'a TimeRecordInput,
        }
        assert_eq!(
            serde_json::to_string(&[Update {
                id: 7,
                input: &input
            }])
            .unwrap(),
            r#"[{"id":7,"projectId":1,"date":"2026-11-06","description":"d","duration":150,"onSite":false}]"#
        );
    }
}
