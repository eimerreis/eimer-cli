//! Shared PR helpers: platform detection, PR target resolution (id/branch/repo),
//! GitHub remote parsing and small process helpers the core does not provide.

use std::process::{Command, Stdio};

use anyhow::{Context, Result, anyhow, bail};
use clap::Args;
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::core::error::{HintExt, Hinted};
use crate::core::process::run_text;
use crate::core::prompt;
use crate::core::ui::{self, symbols};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Platform {
    Github,
    AzureDevops,
}

impl Platform {
    /// Raw identifier, as serialized to JSON.
    pub fn id(self) -> &'static str {
        match self {
            Platform::Github => "github",
            Platform::AzureDevops => "azure-devops",
        }
    }
}

/// Detects the hosting platform from a git remote URL.
pub fn detect_platform(remote_url: &str) -> Result<Platform> {
    let normalized = remote_url.to_lowercase();
    if normalized.contains("github.com") {
        return Ok(Platform::Github);
    }
    if normalized.contains("dev.azure.com")
        || normalized.contains("ssh.dev.azure.com")
        || normalized.contains("vs-ssh.visualstudio.com")
    {
        return Ok(Platform::AzureDevops);
    }
    bail!("Unsupported remote URL: {remote_url}")
}

/// `origin` remote URL of the cwd's repository.
pub fn origin_url() -> Result<String> {
    Ok(run_text(["git", "remote", "get-url", "origin"])?
        .trim()
        .to_string())
}

/// `--id` wins; otherwise the positional is parsed like JS `parseInt` (leading digits).
/// Non-numeric or non-positive positionals are ignored.
pub fn resolve_id_arg(flag: Option<u64>, positional: Option<&str>) -> Option<u64> {
    if let Some(id) = flag.filter(|&id| id > 0) {
        return Some(id);
    }
    let raw = positional?.trim();
    let raw = raw.strip_prefix('+').unwrap_or(raw);
    let digits: String = raw.chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse::<u64>().ok().filter(|&n| n > 0)
}

/// Lookup flags shared by `comments`, `copy`, `open` and `show`.
#[derive(Args)]
pub struct TargetArgs {
    /// Pull request ID/number (defaults to the PR of the current branch)
    #[arg(value_name = "ID")]
    pub positional: Option<String>,
    /// Pull request ID/number override
    #[arg(long, short = 'i', value_parser = clap::value_parser!(u64).range(1..))]
    pub id: Option<u64>,
    /// Branch override for PR lookup
    #[arg(long, short = 'b')]
    pub branch: Option<String>,
    /// Repo override (GitHub: owner/repo, AzDO: repo name)
    #[arg(long, short = 'r')]
    pub repo: Option<String>,
}

/// Resolved PR lookup inputs.
pub struct Target {
    pub remote_url: String,
    pub platform: Platform,
    pub pr_id: Option<u64>,
    /// Empty when a PR ID was given and no `--branch`.
    pub branch: String,
    pub repo: Option<String>,
}

impl Target {
    /// "ID '42'" or "branch 'feature/x'", for not-found messages.
    pub fn lookup_label(&self) -> String {
        match self.pr_id {
            Some(id) => format!("ID '{id}'"),
            None => format!("branch '{}'", self.branch),
        }
    }
}

impl TargetArgs {
    /// Remote + platform, then PR id (flag/positional), else branch (flag, current, prompt).
    pub fn resolve(&self) -> Result<Target> {
        let remote_url = origin_url()?;
        let platform = detect_platform(&remote_url)?;
        let pr_id = resolve_id_arg(self.id, self.positional.as_deref());
        let mut branch = non_empty(self.branch.as_deref()).unwrap_or_default();

        if branch.is_empty() && pr_id.is_none() {
            branch = run_text(["git", "branch", "--show-current"])?
                .trim()
                .to_string();
        }
        if branch.is_empty() && pr_id.is_none() {
            branch = prompt::text("Branch name", None, required("Branch is required"))
                .unwrap_or_default();
            if branch.is_empty() {
                return Err(anyhow!(
                    "Could not determine branch. Pass [id], --branch, or --id."
                ))
                .hint("Run from a checked out branch or pass --branch explicitly.");
            }
        }

        Ok(Target {
            remote_url,
            platform,
            pr_id,
            branch,
            repo: non_empty(self.repo.as_deref()),
        })
    }
}

/// Wraps a command's error with `context` and attaches `hint`, unless a more specific
/// hint is already attached further down the chain.
pub fn finish<T>(result: Result<T>, context: &'static str, hint: &str) -> Result<T> {
    result.map_err(|err| {
        let hinted = err.chain().any(|e| e.is::<Hinted>());
        let err = err.context(context);
        if hinted {
            err
        } else {
            Hinted {
                hint: hint.to_string(),
                source: err,
            }
            .into()
        }
    })
}

/// Trimmed value, `None` when missing or blank.
pub fn non_empty(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(String::from)
}

/// Prompt validator rejecting blank input.
pub fn required(message: &'static str) -> impl Fn(&str) -> Option<String> {
    move |value: &str| value.trim().is_empty().then(|| message.to_string())
}

/// Builds an owned argv from string slices.
pub fn argv(parts: &[&str]) -> Vec<String> {
    parts.iter().map(|s| s.to_string()).collect()
}

/// Milliseconds since epoch for an RFC 3339 timestamp; 0 when missing/invalid (like `Date.parse || 0`).
pub fn timestamp(value: Option<&str>) -> i64 {
    value
        .and_then(|v| chrono::DateTime::parse_from_rfc3339(v).ok())
        .map(|d| d.timestamp_millis())
        .unwrap_or(0)
}

/// Strips `refs/heads/` from an Azure DevOps ref name.
pub fn strip_ref_prefix(branch: Option<&str>) -> String {
    let branch = branch.unwrap_or_default();
    branch
        .strip_prefix("refs/heads/")
        .unwrap_or(branch)
        .to_string()
}

/// Warnings collected while a spinner runs; printed afterwards so they neither garble
/// the spinner nor corrupt `--json` stdout (they go to stderr in JSON mode).
#[derive(Default)]
pub struct Warnings(Vec<String>);

impl Warnings {
    pub fn push(&mut self, message: String) {
        self.0.push(message);
    }

    pub fn flush(self, json: bool) {
        for message in self.0 {
            if json {
                eprintln!("{} {message}", symbols::warning());
            } else {
                ui::print_warning(&message, None);
            }
        }
    }
}

/// Runs a command and parses stdout as JSON, accepting the given exit codes.
/// Returns `None` when stdout is empty (e.g. `gh pr checks` with no checks reported).
pub fn run_json_allow_exit_codes<T: DeserializeOwned>(
    cmd: &[String],
    allowed: &[i32],
) -> Result<Option<T>> {
    let (program, args) = cmd.split_first().ok_or_else(|| anyhow!("empty command"))?;
    let output = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .with_context(|| format!("Failed to start '{program}'. Is it installed and on PATH?"))?;
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let code = output.status.code().unwrap_or(-1);
    if !allowed.contains(&code) {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let detail = if stderr.trim().is_empty() {
            stdout.trim().to_string()
        } else {
            stderr.trim().to_string()
        };
        bail!("Command failed ({}): {detail}", cmd.join(" "));
    }
    if stdout.trim().is_empty() {
        return Ok(None);
    }
    serde_json::from_str(&stdout)
        .map(Some)
        .with_context(|| format!("Failed to parse JSON from '{}'", cmd.join(" ")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_platforms() {
        assert_eq!(
            detect_platform("git@github.com:eimerreis/eimer-cli.git").unwrap(),
            Platform::Github
        );
        assert_eq!(
            detect_platform("https://org@dev.azure.com/org/proj/_git/repo").unwrap(),
            Platform::AzureDevops
        );
        assert_eq!(
            detect_platform("git@ssh.dev.azure.com:v3/org/proj/repo").unwrap(),
            Platform::AzureDevops
        );
        assert_eq!(
            detect_platform("org@org.vs-ssh.visualstudio.com:v3/org/proj/repo").unwrap(),
            Platform::AzureDevops
        );
        assert!(detect_platform("https://gitlab.com/a/b").is_err());
    }

    #[test]
    fn resolves_id_args() {
        assert_eq!(resolve_id_arg(Some(7), Some("42")), Some(7));
        assert_eq!(resolve_id_arg(None, Some("42")), Some(42));
        assert_eq!(resolve_id_arg(None, Some(" 42abc ")), Some(42));
        assert_eq!(resolve_id_arg(None, Some("abc")), None);
        assert_eq!(resolve_id_arg(None, Some("0")), None);
        assert_eq!(resolve_id_arg(None, Some("-3")), None);
        assert_eq!(resolve_id_arg(None, None), None);
    }

    #[test]
    fn strips_refs_and_parses_times() {
        assert_eq!(strip_ref_prefix(Some("refs/heads/feat/x")), "feat/x");
        assert_eq!(strip_ref_prefix(Some("feat/x")), "feat/x");
        assert_eq!(strip_ref_prefix(None), "");
        assert_eq!(timestamp(Some("nope")), 0);
        assert!(timestamp(Some("2024-01-02T03:04:05.123456Z")) > 0);
    }
}
