//! Azure DevOps REST client for release commands (port of `azure-client.ts`).
//!
//! Auth: `SYSTEM_ACCESSTOKEN` -> `AZURE_DEVOPS_PAT` -> `az account get-access-token`.
//! Context: `SYSTEM_COLLECTIONURI` + `SYSTEM_TEAMPROJECT` -> `az devops configure --list`
//! -> the cwd's Azure DevOps git remote.

use anyhow::{Result, anyhow, bail};
use reqwest::Url;
use reqwest::blocking::{Client, Response};
use serde::de::DeserializeOwned;

use crate::core::azure::{decode_path_segment, remote_project};
use crate::core::process::run_text;

const AZURE_DEVOPS_RESOURCE: &str = "499b84ac-1321-427f-aa17-267ca6975798";
const AZURE_DEVOPS_API_VERSION: &str = "7.1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AzureContext {
    pub organization_url: String,
    pub project: String,
    pub base_url: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AzureAuth {
    Bearer(String),
    Pat(String),
}

pub type Query<'a> = Vec<(&'a str, Option<String>)>;

pub struct AzureDevOpsClient {
    pub context: AzureContext,
    auth: AzureAuth,
    http: Client,
}

impl AzureDevOpsClient {
    pub fn new(context: AzureContext, auth: AzureAuth) -> Self {
        Self {
            context,
            auth,
            http: crate::core::http::client(),
        }
    }

    /// Resolves context and auth from env vars / `az`.
    pub fn create() -> Result<Self> {
        let env = |name: &str| std::env::var(name).ok();
        let az = |args: &[&str]| run_text(args);
        let context = resolve_context(env, az, remote_project_context)?;
        let auth = resolve_auth(env, az)?;
        Ok(Self::new(context, auth))
    }

    pub fn get_json<T: DeserializeOwned>(&self, path: &str, query: Query) -> Result<T> {
        self.request_json("GET", path, query, None)
    }

    pub fn patch_json<T: DeserializeOwned>(
        &self,
        path: &str,
        body: &serde_json::Value,
        query: Query,
    ) -> Result<T> {
        self.request_json("PATCH", path, query, Some(body))
    }

    fn request_json<T: DeserializeOwned>(
        &self,
        method: &str,
        path: &str,
        query: Query,
        body: Option<&serde_json::Value>,
    ) -> Result<T> {
        let url = build_url(&self.context.base_url, path, query)?;
        let mut request = match method {
            "PATCH" => self.http.patch(url.clone()),
            _ => self.http.get(url.clone()),
        }
        .header("Authorization", authorization_header(&self.auth));
        if let Some(body) = body {
            request = request
                .header("Content-Type", "application/json")
                .body(serde_json::to_string(body)?);
        }

        let response = request
            .send()
            .map_err(|err| anyhow!("Azure DevOps request failed for {url}: {err}"))?;
        let status = response.status();
        let content_type = header(&response, "content-type");
        let final_host = response.url().host_str().unwrap_or_default().to_lowercase();
        let raw = response.text()?;

        if is_html_response(&content_type, &raw) {
            if is_azure_sign_in_page(&final_host, &raw) {
                bail!(
                    "Azure DevOps returned a sign-in page ({}) for {url}; authentication was rejected. Verify SYSTEM_ACCESSTOKEN or AZURE_DEVOPS_PAT if set, otherwise refresh your Azure CLI login and confirm access to this organization.",
                    status.as_u16()
                );
            }
            bail!(
                "Azure DevOps returned HTML instead of JSON ({}) for {url}. Check the organization/project URL and any network proxy.",
                status.as_u16()
            );
        }

        if !status.is_success() {
            bail!(
                "Azure DevOps request failed ({} {}) for {url}: {}",
                status.as_u16(),
                status.canonical_reason().unwrap_or_default(),
                extract_error_message(&raw)
            );
        }

        let raw = if raw.trim().is_empty() { "{}" } else { &raw };
        serde_json::from_str(raw)
            .map_err(|err| anyhow!("Failed to parse Azure DevOps JSON response from {url}: {err}"))
    }
}

fn header(response: &Response, name: &str) -> String {
    response
        .headers()
        .get(name)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_lowercase()
}

fn is_html_response(content_type: &str, raw: &str) -> bool {
    content_type.contains("text/html") || raw.trim_start().starts_with('<')
}

fn is_azure_sign_in_page(final_host: &str, raw: &str) -> bool {
    let lower = raw.to_lowercase();
    let title = lower
        .find("<title")
        .and_then(|start| {
            let rest = &lower[start..];
            let open_end = rest.find('>')? + 1;
            let close = rest[open_end..].find("</title>")?;
            Some(rest[open_end..open_end + close].to_string())
        })
        .unwrap_or_default();
    let title_matches = title
        .find("azure devops")
        .is_some_and(|i| title[i..].contains("sign in"));
    title_matches || final_host.ends_with(".vssps.visualstudio.com")
}

fn extract_error_message(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return "no response body".into();
    }
    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(trimmed) {
        let message = parsed["message"]
            .as_str()
            .filter(|m| !m.is_empty())
            .or_else(|| {
                parsed["error"]["message"]
                    .as_str()
                    .filter(|m| !m.is_empty())
            });
        if let Some(message) = message {
            return message.to_string();
        }
    }
    trimmed.to_string()
}

pub fn build_url(base_url: &str, path: &str, query: Query) -> Result<Url> {
    let base = base_url.strip_suffix('/').unwrap_or(base_url);
    let path = if path.starts_with('/') {
        path.to_string()
    } else {
        format!("/{path}")
    };
    let mut url = Url::parse(&format!("{base}{path}"))
        .map_err(|err| anyhow!("Invalid Azure DevOps URL '{base}{path}': {err}"))?;
    {
        let mut pairs = url.query_pairs_mut();
        let mut has_version = false;
        for (key, value) in &query {
            has_version |= *key == "api-version";
            if let Some(value) = value {
                pairs.append_pair(key, value);
            }
        }
        if !has_version {
            pairs.append_pair("api-version", AZURE_DEVOPS_API_VERSION);
        }
    }
    Ok(url)
}

pub fn authorization_header(auth: &AzureAuth) -> String {
    match auth {
        AzureAuth::Pat(token) => format!("Basic {}", base64(format!(":{token}").as_bytes())),
        AzureAuth::Bearer(token) => format!("Bearer {token}"),
    }
}

fn build_context(organization_url: &str, project: &str) -> AzureContext {
    let org = organization_url
        .strip_suffix('/')
        .unwrap_or(organization_url)
        .to_string();
    let project = project.trim().to_string();
    AzureContext {
        base_url: format!("{org}/{project}"),
        organization_url: org,
        project,
    }
}

fn remote_project_context() -> Option<AzureContext> {
    let remote = remote_project()?;
    let org_url = remote.base_url.rsplit_once('/')?.0.to_string();
    Some(build_context(
        &org_url,
        &decode_path_segment(&remote.project),
    ))
}

fn non_empty(value: Option<String>) -> Option<String> {
    value
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

/// Context precedence: CI env vars, `az devops configure --list`, git remote.
pub fn resolve_context(
    env: impl Fn(&str) -> Option<String>,
    az: impl Fn(&[&str]) -> Result<String>,
    remote: impl Fn() -> Option<AzureContext>,
) -> Result<AzureContext> {
    if let (Some(uri), Some(project)) = (
        non_empty(env("SYSTEM_COLLECTIONURI")),
        non_empty(env("SYSTEM_TEAMPROJECT")),
    ) {
        return Ok(build_context(&uri, &project));
    }

    let az_result = az(&["az", "devops", "configure", "--list"]);
    if let Ok(config) = &az_result {
        let org = config_value(config, "organization");
        let project = config_value(config, "project");
        if let (Some(org), Some(project)) = (org, project) {
            return Ok(build_context(&org, &project));
        }
    }

    if let Some(context) = remote() {
        return Ok(context);
    }

    az_result?;
    bail!(
        "Azure DevOps context missing. Set SYSTEM_COLLECTIONURI + SYSTEM_TEAMPROJECT in CI, or configure local defaults with 'az devops configure -d organization=<url> project=<name>'."
    )
}

/// `key = value` line from `az devops configure --list` (case-insensitive key).
fn config_value(config: &str, key: &str) -> Option<String> {
    config.lines().find_map(|line| {
        let (k, v) = line.split_once('=')?;
        if !k.trim_end().eq_ignore_ascii_case(key) || k.starts_with(char::is_whitespace) {
            return None;
        }
        non_empty(Some(v.to_string()))
    })
}

/// Auth precedence: `SYSTEM_ACCESSTOKEN`, `AZURE_DEVOPS_PAT`, `az account get-access-token`.
pub fn resolve_auth(
    env: impl Fn(&str) -> Option<String>,
    az: impl Fn(&[&str]) -> Result<String>,
) -> Result<AzureAuth> {
    if let Some(token) = non_empty(env("SYSTEM_ACCESSTOKEN")) {
        return Ok(AzureAuth::Bearer(token));
    }
    if let Some(pat) = non_empty(env("AZURE_DEVOPS_PAT")) {
        return Ok(AzureAuth::Pat(pat));
    }
    let token = az(&[
        "az",
        "account",
        "get-access-token",
        "--resource",
        AZURE_DEVOPS_RESOURCE,
        "--query",
        "accessToken",
        "--output",
        "tsv",
    ])?
    .trim()
    .to_string();
    if token.is_empty() {
        bail!(
            "Azure DevOps auth token missing. Set SYSTEM_ACCESSTOKEN (CI), AZURE_DEVOPS_PAT, or run 'az login'."
        );
    }
    Ok(AzureAuth::Bearer(token))
}

/// Standard base64 with padding (for PAT basic auth).
fn base64(input: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(TABLE[((n >> (18 - 6 * i)) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::super::test_server::{Reply, serve};
    use super::*;
    use std::collections::HashMap;

    fn ctx(base: &str) -> AzureContext {
        AzureContext {
            organization_url: base.into(),
            project: "test".into(),
            base_url: base.into(),
        }
    }

    // Port of azure-client.test.ts
    #[test]
    fn sign_in_redirect_reports_rejected_authentication() {
        let (base, _) = serve(2, |req| {
            if req.path.starts_with("/signin") {
                Reply {
                    status: 203,
                    headers: vec![("Content-Type".into(), "text/html; charset=utf-8".into())],
                    body: "<html><title>Azure DevOps Services | Sign In</title></html>".into(),
                }
            } else {
                Reply {
                    status: 302,
                    headers: vec![("Location".into(), "/signin".into())],
                    body: String::new(),
                }
            }
        });
        let client = AzureDevOpsClient::new(ctx(&base), AzureAuth::Bearer("invalid".into()));
        let err = client
            .get_json::<serde_json::Value>("/_apis/build/definitions", vec![])
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("Azure DevOps returned a sign-in page"),
            "{err}"
        );
        assert!(err.contains("authentication was rejected"), "{err}");
    }

    #[test]
    fn json_errors_and_auth_headers() {
        let (base, log) = serve(3, |req| match req.path.as_str() {
            p if p.starts_with("/ok") => Reply {
                status: 200,
                headers: vec![("Content-Type".into(), "application/json".into())],
                body: r#"{"value":[1]}"#.into(),
            },
            p if p.starts_with("/empty") => Reply {
                status: 200,
                headers: vec![],
                body: String::new(),
            },
            _ => Reply {
                status: 404,
                headers: vec![],
                body: r#"{"message":"nope"}"#.into(),
            },
        });
        let client = AzureDevOpsClient::new(ctx(&base), AzureAuth::Pat("secret".into()));
        let ok: serde_json::Value = client
            .get_json("ok", vec![("$top", Some("20".into())), ("x", None)])
            .unwrap();
        assert_eq!(ok["value"][0], 1);
        let empty: serde_json::Value = client
            .patch_json("/empty", &serde_json::json!([{"a": 1}]), vec![])
            .unwrap();
        assert_eq!(empty, serde_json::json!({}));
        let err = client
            .get_json::<serde_json::Value>("/missing", vec![])
            .unwrap_err()
            .to_string();
        assert!(
            err.starts_with("Azure DevOps request failed (404 Not Found) for ")
                && err.ends_with(": nope"),
            "{err}"
        );

        let log = log.lock().unwrap();
        assert_eq!(log[0].path, "/ok?%24top=20&api-version=7.1");
        let auth = log[0].headers.iter().find(|(k, _)| k == "authorization");
        assert_eq!(auth.unwrap().1, "Basic OnNlY3JldA==");
        assert_eq!(log[1].method, "PATCH");
        assert_eq!(log[1].body, r#"[{"a":1}]"#);
    }

    #[test]
    fn builds_urls_like_url_search_params() {
        let url = build_url(
            "https://dev.azure.com/org/My Project/",
            "_apis/build/builds",
            vec![
                ("branchName", Some("refs/heads/release/2.0".into())),
                ("name", Some("a b&c".into())),
            ],
        )
        .unwrap();
        assert_eq!(
            url.as_str(),
            "https://dev.azure.com/org/My%20Project/_apis/build/builds?branchName=refs%2Fheads%2Frelease%2F2.0&name=a+b%26c&api-version=7.1"
        );
    }

    fn env_of(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        move |k| map.get(k).cloned()
    }

    #[test]
    fn auth_resolution_order() {
        let az_token = |_: &[&str]| Ok("az-token\n".to_string());
        let both = env_of(&[("SYSTEM_ACCESSTOKEN", "sys"), ("AZURE_DEVOPS_PAT", "pat")]);
        assert_eq!(
            resolve_auth(&both, az_token).unwrap(),
            AzureAuth::Bearer("sys".into())
        );
        let pat = env_of(&[("SYSTEM_ACCESSTOKEN", "  "), ("AZURE_DEVOPS_PAT", "pat")]);
        assert_eq!(
            resolve_auth(&pat, az_token).unwrap(),
            AzureAuth::Pat("pat".into())
        );
        let none = env_of(&[]);
        assert_eq!(
            resolve_auth(&none, az_token).unwrap(),
            AzureAuth::Bearer("az-token".into())
        );
        let err = resolve_auth(&none, |_: &[&str]| Ok(String::new())).unwrap_err();
        assert!(err.to_string().contains("auth token missing"));
        assert_eq!(base64(b":pat"), "OnBhdA==");
        assert_eq!(base64(b"ab"), "YWI=");
    }

    #[test]
    fn context_resolution_order() {
        let az_list = |_: &[&str]| {
            Ok(
                "[defaults]\norganization = https://dev.azure.com/az-org/\nproject = AzProj\n"
                    .to_string(),
            )
        };
        let remote = || Some(build_context("https://dev.azure.com/git", "Remote"));
        let ci = env_of(&[
            ("SYSTEM_COLLECTIONURI", "https://dev.azure.com/ci-org/"),
            ("SYSTEM_TEAMPROJECT", "Ci Proj"),
        ]);
        let c = resolve_context(&ci, az_list, remote).unwrap();
        assert_eq!(c.base_url, "https://dev.azure.com/ci-org/Ci Proj");
        assert_eq!(c.organization_url, "https://dev.azure.com/ci-org");

        let partial = env_of(&[("SYSTEM_COLLECTIONURI", "https://x")]);
        let c = resolve_context(&partial, az_list, remote).unwrap();
        assert_eq!(c.base_url, "https://dev.azure.com/az-org/AzProj");

        let no_defaults = |_: &[&str]| Ok("[defaults]\norganization =\n".to_string());
        let c = resolve_context(&partial, no_defaults, remote).unwrap();
        assert_eq!(c.base_url, "https://dev.azure.com/git/Remote");

        let err = resolve_context(&partial, no_defaults, || None).unwrap_err();
        assert!(err.to_string().contains("Azure DevOps context missing"));
        let err = resolve_context(&partial, |_: &[&str]| bail!("az missing"), || None).unwrap_err();
        assert_eq!(err.to_string(), "az missing");
    }
}
