//! Shared helpers for `eimer pipeline`: run accessors, status formatting, durations
//! and positional-arg resolution.

use anyhow::Result;
use chrono::{DateTime, NaiveDateTime, Utc};
use serde_json::Value;

pub use super::repo::{build_run_url, get_repo_info};
use crate::core::azure::AzScope;
use crate::core::ui::{self, symbols};

/// `az <parts...> --detect true --output json`.
pub fn az_json(parts: &[&str]) -> Vec<String> {
    let mut cmd: Vec<String> = std::iter::once("az")
        .chain(parts.iter().copied())
        .map(String::from)
        .collect();
    cmd.extend(AzScope::detect().args());
    cmd.extend(["--output".into(), "json".into()]);
    cmd
}

/// Non-empty string at a JSON path (TS `a?.b || fallback` semantics).
pub fn str_at<'a>(value: &'a Value, path: &[&str]) -> Option<&'a str> {
    path.iter()
        .try_fold(value, |v, key| v.get(key))?
        .as_str()
        .filter(|s| !s.is_empty())
}

/// Read-only view over a raw `az pipelines runs` JSON object. The raw value is kept so
/// `--json` output passes az's shape through unchanged.
#[derive(Clone, Copy)]
pub struct Run<'a>(pub &'a Value);

impl<'a> Run<'a> {
    pub fn id(&self) -> u64 {
        self.0.get("id").and_then(Value::as_u64).unwrap_or(0)
    }
    pub fn str(&self, path: &[&str]) -> Option<&'a str> {
        str_at(self.0, path)
    }
    pub fn status(&self) -> Option<&'a str> {
        self.str(&["status"])
    }
    pub fn result(&self) -> Option<&'a str> {
        self.str(&["result"])
    }
    pub fn pipeline_name(&self) -> &'a str {
        self.str(&["definition", "name"])
            .unwrap_or("Unknown pipeline")
    }
    pub fn status_label(&self) -> String {
        format_run_status(self.status(), self.result())
    }
    pub fn duration(&self) -> String {
        calculate_duration(
            self.str(&["startTime"]),
            self.str(&["finishTime"]),
            Utc::now(),
        )
    }
    pub fn link(&self) -> String {
        ui::terminal_link(&format!("#{}", self.id()), &build_run_url(self.id()))
    }

    /// Trigger message, else "Build for PR n", else "Build for <branch>".
    pub fn message(&self) -> String {
        let trigger = self.0.get("triggerInfo");
        let text = |key: &str| -> Option<String> {
            let v = trigger?.get(key)?;
            let s = match v {
                Value::String(s) => s.trim().to_string(),
                Value::Number(n) => n.to_string(),
                _ => return None,
            };
            (!s.is_empty()).then_some(s)
        };
        if let Some(message) = text("ci.message") {
            return message;
        }
        if let Some(pr) = text("pr.number") {
            return format!("Build for PR {pr}");
        }
        let branch = strip_branch_prefix(self.str(&["sourceBranch"]));
        if !branch.is_empty() {
            return format!("Build for {branch}");
        }
        "No trigger message".into()
    }

    /// Whether the run's repository name (or the tail of its id) equals `repo_name`.
    pub fn matches_repo(&self, repo_name: &str) -> bool {
        if self
            .str(&["repository", "name"])
            .is_some_and(|n| n.eq_ignore_ascii_case(repo_name))
        {
            return true;
        }
        self.str(&["repository", "id"])
            .and_then(|id| id.rsplit('/').next())
            .is_some_and(|tail| tail.eq_ignore_ascii_case(repo_name))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunState {
    Running,
    Queued,
    Succeeded,
    Failed,
    Canceled,
    Unknown,
}

pub fn run_state(status: Option<&str>, result: Option<&str>) -> RunState {
    let status = status.unwrap_or_default().to_lowercase();
    let result = result.unwrap_or_default().to_lowercase();
    match (status.as_str(), result.as_str()) {
        ("inprogress", _) => RunState::Running,
        ("notstarted", _) => RunState::Queued,
        ("completed", "succeeded") => RunState::Succeeded,
        ("completed", "failed") => RunState::Failed,
        ("completed", "canceled") => RunState::Canceled,
        _ => RunState::Unknown,
    }
}

/// e.g. "+ SUCCEEDED", "* RUNNING" (colored).
pub fn format_run_status(status: Option<&str>, result: Option<&str>) -> String {
    match run_state(status, result) {
        RunState::Running => format!("{} {}", symbols::running(), ui::info("RUNNING")),
        RunState::Queued => format!("{} {}", symbols::queued(), ui::dim("QUEUED")),
        RunState::Succeeded => format!("{} {}", symbols::ok(), ui::success("SUCCEEDED")),
        RunState::Failed => format!("{} {}", symbols::fail(), ui::error("FAILED")),
        RunState::Canceled => format!("{} {}", symbols::warning(), ui::warning("CANCELED")),
        RunState::Unknown => {
            let label = result
                .or(status)
                .map(str::to_uppercase)
                .unwrap_or_else(|| "UNKNOWN".into());
            format!("{} {}", symbols::neutral(), ui::dim(&label))
        }
    }
}

fn parse_time(value: &str) -> Option<DateTime<Utc>> {
    if let Ok(t) = DateTime::parse_from_rfc3339(value) {
        return Some(t.with_timezone(&Utc));
    }
    // az occasionally omits the offset; JS Date.parse would treat that as local time,
    // UTC is close enough for a duration.
    NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S%.f")
        .ok()
        .map(|t| t.and_utc())
}

/// "Not started", "42s", "3m 5s", "3m 5s (running)".
pub fn calculate_duration(start: Option<&str>, finish: Option<&str>, now: DateTime<Utc>) -> String {
    let Some(start) = start.filter(|s| !s.is_empty()) else {
        return "Not started".into();
    };
    let Some(start) = parse_time(start) else {
        return "Unknown".into();
    };
    let finish = finish.filter(|s| !s.is_empty());
    let end = finish.and_then(parse_time).unwrap_or(now);
    let elapsed = (end - start).num_seconds().max(0);
    let (minutes, seconds) = (elapsed / 60, elapsed % 60);
    let duration = if minutes > 0 {
        format!("{minutes}m {seconds}s")
    } else {
        format!("{seconds}s")
    };
    if finish.is_none() {
        format!("{duration} (running)")
    } else {
        duration
    }
}

pub fn strip_branch_prefix(value: Option<&str>) -> &str {
    let value = value.unwrap_or_default();
    value.strip_prefix("refs/heads/").unwrap_or(value)
}

/// `key=value` entries, trimmed; entries without `=` are dropped.
pub fn parse_key_value_pairs(values: &[String]) -> Vec<String> {
    values
        .iter()
        .map(|v| v.trim())
        .filter(|v| v.contains('='))
        .map(String::from)
        .collect()
}

/// Flag wins; otherwise the positional parsed like JS `parseInt` (leading digits, > 0).
pub fn resolve_id_arg(flag: Option<u64>, positional: Option<&str>) -> Option<u64> {
    if flag.is_some() {
        return flag;
    }
    let raw = positional?.trim();
    let digits: String = raw.chars().take_while(char::is_ascii_digit).collect();
    digits.parse::<u64>().ok().filter(|n| *n > 0)
}

/// Trimmed flag if non-empty, else positionals joined by spaces.
pub fn resolve_string_arg(flag: Option<&str>, positional: &[String]) -> String {
    match flag.map(str::trim).filter(|f| !f.is_empty()) {
        Some(flag) => flag.to_string(),
        None => positional.join(" ").trim().to_string(),
    }
}

/// Fetches the 100 most recent runs and returns the latest one for `repo_name`.
pub fn latest_repo_run_id(repo_name: &str) -> Result<Option<u64>> {
    let runs: Vec<Value> = crate::core::process::run_json(az_json(&[
        "pipelines",
        "runs",
        "list",
        "--top",
        "100",
        "--query-order",
        "QueueTimeDesc",
    ]))?;
    Ok(runs
        .iter()
        .map(Run)
        .find(|r| r.matches_repo(repo_name))
        .map(|r| r.id()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn states() {
        assert_eq!(run_state(Some("inProgress"), None), RunState::Running);
        assert_eq!(run_state(Some("notStarted"), None), RunState::Queued);
        assert_eq!(
            run_state(Some("completed"), Some("succeeded")),
            RunState::Succeeded
        );
        assert_eq!(
            run_state(Some("Completed"), Some("Failed")),
            RunState::Failed
        );
        assert_eq!(
            run_state(Some("completed"), Some("canceled")),
            RunState::Canceled
        );
        assert_eq!(
            run_state(Some("completed"), Some("partiallySucceeded")),
            RunState::Unknown
        );
        assert_eq!(run_state(None, None), RunState::Unknown);
    }

    #[test]
    fn status_labels_without_color() {
        // Tests run without a TTY, so colors are off.
        assert_eq!(
            format_run_status(Some("completed"), Some("succeeded")),
            "+ SUCCEEDED"
        );
        assert_eq!(format_run_status(Some("inProgress"), None), "* RUNNING");
        assert_eq!(
            format_run_status(Some("completed"), Some("partiallySucceeded")),
            "- PARTIALLYSUCCEEDED"
        );
        assert_eq!(format_run_status(Some("cancelling"), None), "- CANCELLING");
        assert_eq!(format_run_status(None, None), "- UNKNOWN");
    }

    #[test]
    fn durations() {
        let now = DateTime::parse_from_rfc3339("2024-01-01T10:10:00Z")
            .unwrap()
            .with_timezone(&Utc);
        assert_eq!(calculate_duration(None, None, now), "Not started");
        assert_eq!(calculate_duration(Some(""), None, now), "Not started");
        assert_eq!(calculate_duration(Some("garbage"), None, now), "Unknown");
        assert_eq!(
            calculate_duration(
                Some("2024-01-01T10:00:00.1234567Z"),
                Some("2024-01-01T10:03:05.5+00:00"),
                now
            ),
            "3m 5s"
        );
        assert_eq!(
            calculate_duration(
                Some("2024-01-01T10:00:00Z"),
                Some("2024-01-01T10:00:42Z"),
                now
            ),
            "42s"
        );
        assert_eq!(
            calculate_duration(Some("2024-01-01T10:09:30Z"), None, now),
            "30s (running)"
        );
        assert_eq!(
            calculate_duration(Some("2024-01-01T10:00:00"), Some("bad"), now),
            "10m 0s"
        );
    }

    #[test]
    fn messages() {
        let msg = |v: Value| Run(&v).message();
        assert_eq!(
            msg(json!({"triggerInfo": {"ci.message": "  fix it "}})),
            "fix it"
        );
        assert_eq!(
            msg(json!({"triggerInfo": {"ci.message": " ", "pr.number": "42"}})),
            "Build for PR 42"
        );
        assert_eq!(
            msg(json!({"sourceBranch": "refs/heads/main"})),
            "Build for main"
        );
        assert_eq!(
            msg(json!({"sourceBranch": "refs/pull/1/merge"})),
            "Build for refs/pull/1/merge"
        );
        assert_eq!(msg(json!({})), "No trigger message");
    }

    #[test]
    fn repo_matching() {
        let run = json!({"repository": {"name": "App", "id": "x"}});
        assert!(Run(&run).matches_repo("app"));
        let gh = json!({"repository": {"name": null, "id": "eimerreis/eimer-cli"}});
        assert!(Run(&gh).matches_repo("eimer-cli"));
        assert!(!Run(&gh).matches_repo("other"));
        assert!(!Run(&json!({})).matches_repo("x"));
    }

    #[test]
    fn args() {
        assert_eq!(resolve_id_arg(Some(5), Some("7")), Some(5));
        assert_eq!(resolve_id_arg(None, Some(" 7 ")), Some(7));
        assert_eq!(resolve_id_arg(None, Some("12abc")), Some(12));
        assert_eq!(resolve_id_arg(None, Some("0")), None);
        assert_eq!(resolve_id_arg(None, Some("abc")), None);
        assert_eq!(resolve_id_arg(None, None), None);

        let pos = vec!["deploy".to_string(), "prod".to_string()];
        assert_eq!(resolve_string_arg(Some(" x "), &pos), "x");
        assert_eq!(resolve_string_arg(Some(" "), &pos), "deploy prod");
        assert_eq!(resolve_string_arg(None, &[]), "");

        let kv = vec![" a=1 ".into(), "b".into(), "".into(), "c=".into()];
        assert_eq!(parse_key_value_pairs(&kv), vec!["a=1", "c="]);
    }

    #[test]
    fn az_command_shape() {
        assert_eq!(
            az_json(&["pipelines", "show", "--id", "3"]),
            vec![
                "az",
                "pipelines",
                "show",
                "--id",
                "3",
                "--detect",
                "true",
                "--output",
                "json"
            ]
        );
    }
}
