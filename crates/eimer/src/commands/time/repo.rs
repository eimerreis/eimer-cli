//! Repo-level `.time.json` (time projects this repo bills to, with the invoices written for
//! them) and the PR it acts on.

use std::path::PathBuf;

use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};

use crate::core::error::HintExt;
use crate::core::process::{run_json, run_text, run_with_stdin};

pub const CONFIG_FILE: &str = ".time.json";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TimeConfig {
    pub projects: Vec<ProjectConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectConfig {
    pub alias: String,
    pub id: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Invoices written for this project, oldest first (see `eimer time invoiced`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub invoices: Vec<Invoice>,
}

/// One invoice: it settles every record up to and including `through`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Invoice {
    /// Last work day the invoice covers, yyyy-mm-dd.
    pub through: String,
    /// Invoice date, yyyy-mm-dd.
    pub date: String,
    /// Invoice number in the billing tool, e.g. Lexoffice.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub number: Option<String>,
}

impl ProjectConfig {
    pub fn last_invoice(&self) -> Option<&Invoice> {
        self.invoices.last()
    }

    /// Appends an invoice; it must cover days after the previous one.
    pub fn add_invoice(&mut self, invoice: Invoice) -> Result<()> {
        if let Some(last) = self.last_invoice()
            && invoice.through <= last.through
        {
            return Err(anyhow!(
                "The last invoice already covers work through {}",
                last.through
            ))
            .hint("Pass a later --through date, or fix .time.json by hand");
        }
        self.invoices.push(invoice);
        Ok(())
    }
}

/// Top level of the current worktree.
pub fn root() -> Result<PathBuf> {
    let root = run_text(["git", "rev-parse", "--show-toplevel"])
        .context("Failed to find the git repository")?;
    Ok(PathBuf::from(root.trim()))
}

fn config_path() -> Result<PathBuf> {
    Ok(root()?.join(CONFIG_FILE))
}

pub fn read_config() -> Result<Option<TimeConfig>> {
    let path = config_path()?;
    match std::fs::read_to_string(&path) {
        Ok(raw) => {
            Ok(Some(serde_json::from_str(&raw).with_context(|| {
                format!("Failed to parse {CONFIG_FILE}")
            })?))
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err).with_context(|| format!("Failed to read {CONFIG_FILE}")),
    }
}

pub fn load_config() -> Result<TimeConfig> {
    let config = read_config()?
        .ok_or_else(|| anyhow!("No {CONFIG_FILE} in this repo"))
        .hint("Run `eimer time projects`, then `eimer time init <projectId>`.")?;
    if config.projects.is_empty() {
        bail!("{CONFIG_FILE} has no projects");
    }
    Ok(config)
}

pub fn save_config(config: &TimeConfig) -> Result<()> {
    let raw = serde_json::to_string_pretty(config)? + "\n";
    std::fs::write(config_path()?, raw).with_context(|| format!("Failed to write {CONFIG_FILE}"))
}

impl TimeConfig {
    /// With a single project the alias is optional; with several it is required.
    pub fn resolve(&self, alias: Option<&str>) -> Result<&ProjectConfig> {
        let known = || {
            self.projects
                .iter()
                .map(|p| p.alias.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        };
        match (alias, self.projects.as_slice()) {
            (Some(alias), _) => self
                .projects
                .iter()
                .find(|p| p.alias == alias)
                .ok_or_else(|| anyhow!("Unknown project @{alias}"))
                .hint(format!("Known projects: {}", known())),
            (None, [only]) => Ok(only),
            (None, _) => Err(anyhow!("This repo bills to several time projects"))
                .hint(format!("Pass --project <alias> ({})", known())),
        }
    }
}

#[derive(Debug, Clone)]
pub struct PullRequest {
    pub number: u64,
    pub body: String,
    pub url: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GhPr {
    number: u64,
    #[serde(default)]
    body: String,
    url: String,
}

/// The given PR, or the PR of the current branch.
pub fn pull_request(pr: Option<u64>) -> Result<PullRequest> {
    let mut cmd = vec!["gh".to_string(), "pr".into(), "view".into()];
    cmd.extend(pr.map(|n| n.to_string()));
    cmd.extend(["--json".into(), "number,body,url".into()]);
    let data: GhPr = run_json(&cmd)
        .context(match pr {
            Some(n) => format!("Failed to load PR #{n}"),
            None => "Failed to find a PR for the current branch".into(),
        })
        .hint("Open the PR first or pass --pr <number>.")?;
    Ok(PullRequest {
        number: data.number,
        body: data.body,
        url: data.url,
    })
}

#[derive(Debug, Deserialize)]
pub struct PrFile {
    pub filename: String,
    /// added, removed, modified, renamed, ...
    pub status: String,
}

pub fn pr_files(number: u64) -> Result<Vec<PrFile>> {
    let raw = run_text([
        "gh",
        "api",
        "--paginate",
        "--jq",
        ".[] | {filename, status}",
        &format!("repos/{{owner}}/{{repo}}/pulls/{number}/files"),
    ])
    .with_context(|| format!("Failed to list the files of PR #{number}"))?;
    raw.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).context("Failed to parse PR files"))
        .collect()
}

/// `.time/<name>.md` (not the README).
pub fn is_pending_entry(path: &str) -> bool {
    path.strip_prefix(".time/")
        .is_some_and(|name| name.ends_with(".md") && !name.contains('/') && name != "README.md")
}

/// Files a booking PR may touch.
pub fn is_booking_file(path: &str) -> bool {
    path.starts_with(".time/") || path == "TIME.md" || path.starts_with(".changeset/")
}

pub fn set_body(number: u64, body: &str) -> Result<()> {
    run_with_stdin(
        ["gh", "pr", "edit", &number.to_string(), "--body-file", "-"],
        body,
    )
    .with_context(|| format!("Failed to update the body of PR #{number}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(aliases: &[&str]) -> TimeConfig {
        TimeConfig {
            projects: aliases
                .iter()
                .enumerate()
                .map(|(i, a)| ProjectConfig {
                    alias: (*a).into(),
                    id: i as u64 + 1,
                    name: None,
                    invoices: vec![],
                })
                .collect(),
        }
    }

    #[test]
    fn resolves_projects() {
        assert_eq!(config(&["web"]).resolve(None).unwrap().id, 1);
        assert_eq!(config(&["web", "app"]).resolve(Some("app")).unwrap().id, 2);
        assert!(config(&["web", "app"]).resolve(None).is_err());
        assert!(config(&["web"]).resolve(Some("nope")).is_err());
    }

    fn invoice(through: &str) -> Invoice {
        Invoice {
            through: through.into(),
            date: "2026-11-17".into(),
            number: None,
        }
    }

    #[test]
    fn invoices_must_move_forward() {
        let mut project = config(&["web"]).projects.remove(0);
        project.add_invoice(invoice("2026-10-16")).unwrap();
        project.add_invoice(invoice("2026-11-16")).unwrap();
        let err = project.add_invoice(invoice("2026-11-16")).unwrap_err();
        assert_eq!(
            err.to_string(),
            "The last invoice already covers work through 2026-11-16"
        );
        assert_eq!(project.last_invoice(), Some(&invoice("2026-11-16")));
    }

    #[test]
    fn reads_config_without_invoices_and_writes_them_only_when_present() {
        let config: TimeConfig =
            serde_json::from_str(r#"{"projects":[{"alias":"web","id":7}]}"#).unwrap();
        assert!(config.projects[0].invoices.is_empty());
        assert_eq!(
            serde_json::to_string(&config).unwrap(),
            r#"{"projects":[{"alias":"web","id":7}]}"#
        );

        let mut project = config.projects[0].clone();
        project
            .add_invoice(Invoice {
                number: Some("RE0042".into()),
                ..invoice("2026-11-16")
            })
            .unwrap();
        assert_eq!(
            serde_json::to_string(&project).unwrap(),
            r#"{"alias":"web","id":7,"invoices":[{"through":"2026-11-16","date":"2026-11-17","number":"RE0042"}]}"#
        );
    }

    #[test]
    fn classifies_time_files() {
        assert!(is_pending_entry(".time/2026-11-12-ab12cd.md"));
        assert!(!is_pending_entry(".time/README.md"));
        assert!(!is_pending_entry(".time/x/y.md"));
        assert!(!is_pending_entry("src/.time/a.md"));
        assert!(is_booking_file("TIME.md"));
        assert!(is_booking_file(".changeset/x.md"));
        assert!(!is_booking_file("src/main.rs"));
    }
}
