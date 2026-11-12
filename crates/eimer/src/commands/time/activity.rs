//! Agent activity per repo, read from local agent transcripts (Claude Code, Codex).
//!
//! Every transcript line carries a timestamp. Events closer than the idle gap form
//! one active span; spans of parallel sessions are merged so time never counts twice.
//! T3 Code runs Claude Code and Codex, so its sessions land in the same transcripts.

use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::core::process::try_run_text;

/// The repo whose activity is measured: all its worktrees share one git common dir.
pub struct RepoIdentity {
    common_dir: PathBuf,
    name: String,
}

impl RepoIdentity {
    pub fn current() -> Result<Self> {
        let common_dir =
            git_common_dir(Path::new(".")).context("Failed to find the git repository")?;
        let name = repo_name(&common_dir);
        Ok(Self { common_dir, name })
    }

    /// Key for per-repo local state.
    fn key(&self) -> String {
        self.common_dir.display().to_string()
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    /// Whether a transcript's working directory belongs to this repo. Deleted T3
    /// worktrees (`~/.t3/worktrees/<repo>/<name>`) are matched by path.
    fn contains(&self, cwd: &Path, cache: &mut HashMap<PathBuf, bool>) -> bool {
        if let Some(&hit) = cache.get(cwd) {
            return hit;
        }
        let hit = match git_common_dir(cwd) {
            Some(dir) => dir == self.common_dir,
            None => is_t3_worktree_of(cwd, &self.name),
        };
        cache.insert(cwd.to_path_buf(), hit);
        hit
    }
}

fn git_common_dir(dir: &Path) -> Option<PathBuf> {
    if !dir.is_dir() {
        return None;
    }
    let raw = try_run_text([
        "git",
        "-C",
        &dir.to_string_lossy(),
        "rev-parse",
        "--path-format=absolute",
        "--git-common-dir",
    ])?;
    std::fs::canonicalize(raw.trim()).ok()
}

/// `/x/lauterbad-gmbh/.git` → `lauterbad-gmbh`.
fn repo_name(common_dir: &Path) -> String {
    let dir = if common_dir.file_name().is_some_and(|n| n == ".git") {
        common_dir.parent().unwrap_or(common_dir)
    } else {
        common_dir
    };
    dir.file_name()
        .map(|n| n.to_string_lossy().trim_end_matches(".git").to_string())
        .unwrap_or_default()
}

fn is_t3_worktree_of(cwd: &Path, repo: &str) -> bool {
    let parts: Vec<_> = cwd
        .components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect();
    parts
        .windows(3)
        .any(|w| w[0] == ".t3" && w[1] == "worktrees" && w[2] == repo)
}

/// Per-repo end of the last recorded window (`eimer time add`), so the next
/// suggestion starts where the previous entry ended. Shared by all worktrees.
fn state_path() -> Result<PathBuf> {
    Ok(crate::core::config::config_dir()?.join("time-activity.json"))
}

fn read_state() -> Result<HashMap<String, i64>> {
    match std::fs::read_to_string(state_path()?) {
        Ok(raw) => Ok(serde_json::from_str(&raw).unwrap_or_default()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(HashMap::new()),
        Err(err) => Err(err).context("Failed to read time activity state"),
    }
}

pub fn covered_until(repo: &RepoIdentity) -> Result<Option<i64>> {
    Ok(read_state()?.get(&repo.key()).copied())
}

pub fn set_covered_until(repo: &RepoIdentity, ts: i64) -> Result<()> {
    let mut state = read_state()?;
    state.insert(repo.key(), ts);
    let path = state_path()?;
    std::fs::create_dir_all(path.parent().expect("config dir has a parent"))?;
    std::fs::write(&path, serde_json::to_string_pretty(&state)? + "\n")
        .context("Failed to write time activity state")
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Span {
    /// Unix seconds.
    pub start: i64,
    pub end: i64,
}

/// Collapses events into active spans: a gap longer than `idle` starts a new span.
/// `streams` are per-session event lists; spans of all sessions are merged.
pub fn spans(streams: &[Vec<i64>], idle: i64) -> Vec<Span> {
    let mut spans = Vec::new();
    for stream in streams {
        let mut times = stream.clone();
        times.sort_unstable();
        let Some((&first, rest)) = times.split_first() else {
            continue;
        };
        let (mut start, mut previous) = (first, first);
        for &time in rest {
            if time - previous > idle {
                spans.push(Span {
                    start,
                    end: previous,
                });
                start = time;
            }
            previous = time;
        }
        spans.push(Span {
            start,
            end: previous,
        });
    }
    merge(spans)
}

fn merge(mut spans: Vec<Span>) -> Vec<Span> {
    spans.sort_by_key(|s| s.start);
    let mut merged: Vec<Span> = Vec::new();
    for span in spans {
        match merged.last_mut() {
            Some(last) if span.start <= last.end => last.end = last.end.max(span.end),
            _ => merged.push(span),
        }
    }
    merged
}

/// Seconds of `spans` inside `[from, to)`.
pub fn seconds_within(spans: &[Span], from: i64, to: i64) -> i64 {
    spans
        .iter()
        .map(|s| (s.end.min(to) - s.start.max(from)).max(0))
        .sum()
}

/// One transcript of this repo: all event times and the times of the user's own prompts.
#[derive(Debug, Default, PartialEq)]
pub struct Session {
    pub events: Vec<i64>,
    pub prompts: Vec<i64>,
}

/// Time counted before each prompt for reading the answer and writing the prompt.
pub const PROMPT_LEAD_SECS: i64 = 2 * 60;

/// Focus spans: the user is on a session from one prompt to the next when they
/// follow each other within `gap`; otherwise (the agent worked alone, or the user was
/// away) only `PROMPT_LEAD_SECS` before the next prompt counts.
pub fn focus_spans(sessions: &[Session], gap: i64) -> Vec<Span> {
    let mut spans = Vec::new();
    for session in sessions {
        let mut prompts = session.prompts.clone();
        prompts.sort_unstable();
        for (i, &prompt) in prompts.iter().enumerate() {
            spans.push(Span {
                start: prompt - PROMPT_LEAD_SECS,
                end: prompt,
            });
            if let Some(&next) = prompts.get(i + 1)
                && next - prompt <= gap
            {
                spans.push(Span {
                    start: prompt,
                    end: next,
                });
            }
        }
    }
    merge(spans)
}

/// Transcripts of this repo touched since `since` (unix seconds).
pub fn collect(repo: &RepoIdentity, since: i64) -> Result<Vec<Session>> {
    let home = crate::core::config::home_dir()?;
    let mut files = Vec::new();
    for root in [home.join(".claude/projects"), home.join(".codex/sessions")] {
        jsonl_files(&root, since, &mut files);
    }
    let mut cache = HashMap::new();
    let mut sessions = Vec::new();
    for file in files {
        if let Some(session) = read_transcript(&file, repo, since, &mut cache) {
            sessions.push(session);
        }
    }
    Ok(sessions)
}

/// `*.jsonl` below `dir`, skipping files untouched since `since`.
fn jsonl_files(dir: &Path, since: i64, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(meta) = entry.metadata() else { continue };
        if meta.is_dir() {
            jsonl_files(&path, since, out);
        } else if path.extension().is_some_and(|e| e == "jsonl") && modified(&meta) >= since {
            out.push(path);
        }
    }
}

fn modified(meta: &std::fs::Metadata) -> i64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs() as i64)
}

/// The fields this module reads from both transcript formats.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Line {
    timestamp: Option<String>,
    #[serde(rename = "type")]
    kind: Option<String>,
    /// Claude Code: on most lines.
    cwd: Option<String>,
    /// Claude Code: user turn content.
    message: Option<Message>,
    #[serde(default)]
    is_meta: bool,
    #[serde(default)]
    is_sidechain: bool,
    #[serde(default)]
    is_compact_summary: bool,
    /// Claude Code: "system" for injected prompts (task notifications).
    prompt_source: Option<String>,
    /// Codex: `session_meta`/`turn_context` (cwd) and `response_item` (messages).
    payload: Option<Payload>,
}

#[derive(Deserialize)]
struct Message {
    content: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct Payload {
    cwd: Option<String>,
    #[serde(rename = "type")]
    kind: Option<String>,
    role: Option<String>,
    content: Option<serde_json::Value>,
}

/// Prompts that tools, not the user, send; they do not count as interaction.
const MACHINE_PROMPTS: &[&str] = &[
    "<task-notification>",
    "<local-command-stdout>",
    "<local-command-caveat>",
    "<system-reminder>",
    "<environment_context>",
    "<user_instructions>",
    "<turn_aborted>",
    "# AGENTS.md instructions",
    "Update on pull request #",
    "Context handoff (",
    "Delegated task node:",
];

/// Sessions whose first prompt starts like this are run by another agent or the
/// harness (subagents, title and branch name generation) and are skipped.
const MACHINE_SESSIONS: &[&str] = &[
    "Delegated task node:",
    "Generate a title that",
    "You generate concise git branch",
];

fn is_machine_session(first_prompt: &str) -> bool {
    MACHINE_SESSIONS.iter().any(|p| first_prompt.starts_with(p))
        || (first_prompt.starts_with("Act as the ") && first_prompt.contains("sub-agent"))
}

/// Text of a user turn typed by the user; `None` for tool results and injected text.
fn user_prompt(line: &Line) -> Option<String> {
    let content = match line.kind.as_deref() {
        Some("user") => {
            if line.is_meta
                || line.is_sidechain
                || line.is_compact_summary
                || line.prompt_source.as_deref() == Some("system")
            {
                return None;
            }
            line.message.as_ref()?.content.as_ref()?
        }
        Some("response_item") => {
            let payload = line.payload.as_ref()?;
            if payload.kind.as_deref() != Some("message") || payload.role.as_deref() != Some("user")
            {
                return None;
            }
            payload.content.as_ref()?
        }
        _ => return None,
    };
    let text = match content {
        serde_json::Value::String(text) => text.clone(),
        serde_json::Value::Array(blocks) => {
            if blocks
                .iter()
                .any(|b| b.get("type").and_then(|t| t.as_str()) == Some("tool_result"))
            {
                return None;
            }
            blocks
                .iter()
                .filter_map(|b| b.get("text").and_then(|t| t.as_str()))
                .collect::<Vec<_>>()
                .join(" ")
        }
        _ => return None,
    };
    let text = text.trim().to_string();
    (!MACHINE_PROMPTS.iter().any(|p| text.starts_with(p))).then_some(text)
}

/// The session of a transcript that belongs to `repo`; `None` for other repos and
/// for sessions another agent runs.
fn read_transcript(
    path: &Path,
    repo: &RepoIdentity,
    since: i64,
    cache: &mut HashMap<PathBuf, bool>,
) -> Option<Session> {
    let reader = BufReader::new(std::fs::File::open(path).ok()?);
    let mut matched = false;
    let mut first_prompt_seen = false;
    let mut session = Session::default();
    for raw in reader.lines() {
        let Ok(raw) = raw else { break };
        let Ok(line) = serde_json::from_str::<Line>(&raw) else {
            continue;
        };
        if !matched {
            let cwd = line
                .cwd
                .clone()
                .or_else(|| line.payload.as_ref().and_then(|p| p.cwd.clone()));
            if let Some(cwd) = cwd {
                if !repo.contains(Path::new(cwd.trim_start_matches("file://")), cache) {
                    return None;
                }
                matched = true;
            }
        }
        let Some(ts) = line.timestamp.as_deref().and_then(parse_timestamp) else {
            continue;
        };
        if let Some(prompt) = user_prompt(&line) {
            if !first_prompt_seen && is_machine_session(&prompt) {
                return None;
            }
            first_prompt_seen = true;
            if ts >= since {
                session.prompts.push(ts);
            }
        }
        if ts >= since {
            session.events.push(ts);
        }
    }
    matched.then_some(session)
}

fn parse_timestamp(value: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|d| d.timestamp())
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIN: i64 = 60;

    #[test]
    fn splits_spans_on_idle_gaps() {
        let stream = vec![0, 5 * MIN, 10 * MIN, 60 * MIN, 70 * MIN];
        assert_eq!(
            spans(&[stream], 15 * MIN),
            vec![
                Span {
                    start: 0,
                    end: 10 * MIN
                },
                Span {
                    start: 60 * MIN,
                    end: 70 * MIN
                }
            ]
        );
    }

    #[test]
    fn merges_parallel_sessions() {
        let a = vec![0, 10 * MIN, 20 * MIN];
        let b = vec![15 * MIN, 30 * MIN];
        let merged = spans(&[a, b], 15 * MIN);
        assert_eq!(
            merged,
            vec![Span {
                start: 0,
                end: 30 * MIN
            }]
        );
    }

    #[test]
    fn clips_seconds_to_window() {
        let s = vec![Span {
            start: 0,
            end: 60 * MIN,
        }];
        assert_eq!(seconds_within(&s, 30 * MIN, 120 * MIN), 30 * MIN);
        assert_eq!(seconds_within(&s, 90 * MIN, 120 * MIN), 0);
    }

    #[test]
    fn names_repos_and_matches_t3_worktrees() {
        assert_eq!(
            repo_name(Path::new("/x/lauterbad-gmbh/.git")),
            "lauterbad-gmbh"
        );
        assert_eq!(repo_name(Path::new("/x/bare.git")), "bare");
        assert!(is_t3_worktree_of(
            Path::new("/Users/m/.t3/worktrees/lauterbad-gmbh/t3-1234"),
            "lauterbad-gmbh"
        ));
        assert!(!is_t3_worktree_of(
            Path::new("/Users/m/.t3/worktrees/other/t3-1234"),
            "lauterbad-gmbh"
        ));
    }

    #[test]
    fn counts_focus_between_close_prompts_only() {
        let session = Session {
            events: vec![],
            // quick back-and-forth, then the agent builds for 30 minutes alone
            prompts: vec![60 * MIN, 63 * MIN, 70 * MIN, 100 * MIN],
        };
        assert_eq!(
            focus_spans(&[session], 10 * MIN),
            vec![
                Span {
                    start: 58 * MIN,
                    end: 70 * MIN
                },
                Span {
                    start: 98 * MIN,
                    end: 100 * MIN
                }
            ]
        );
    }

    #[test]
    fn filters_machine_prompts_and_sessions() {
        let line = |json: &str| serde_json::from_str::<Line>(json).unwrap();
        assert_eq!(
            user_prompt(&line(r#"{"type":"user","message":{"content":"fix it"}}"#)).as_deref(),
            Some("fix it")
        );
        for json in [
            r#"{"type":"user","message":{"content":[{"type":"tool_result","content":"x"}]}}"#,
            r#"{"type":"user","isMeta":true,"message":{"content":"x"}}"#,
            r#"{"type":"user","promptSource":"system","message":{"content":"x"}}"#,
            r#"{"type":"user","message":{"content":"<task-notification>x"}}"#,
            r#"{"type":"assistant","message":{"content":"x"}}"#,
            r#"{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"<environment_context>"}]}}"#,
            r#"{"type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"hi"}]}}"#,
        ] {
            assert_eq!(user_prompt(&line(json)), None, "{json}");
        }
        assert_eq!(
            user_prompt(&line(r#"{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"go"}]}}"#)).as_deref(),
            Some("go")
        );
        assert!(is_machine_session("Act as the test sub-agent for x"));
        assert!(is_machine_session("Generate a title that will help"));
        assert!(!is_machine_session("Act as if this was prod"));
    }

    #[test]
    fn reads_claude_and_codex_lines() {
        let dir = tempfile::tempdir().unwrap();
        let repo = RepoIdentity {
            common_dir: PathBuf::from("/nonexistent/.git"),
            name: "demo".into(),
        };
        let claude = dir.path().join("c.jsonl");
        std::fs::write(
            &claude,
            "{\"type\":\"session-owner\"}\n\
             {\"timestamp\":\"2026-11-12T10:00:00Z\",\"type\":\"user\",\"cwd\":\"/u/.t3/worktrees/demo/t3-1\",\"message\":{\"content\":\"go\"}}\n\
             {\"timestamp\":\"2026-11-12T10:05:00Z\",\"type\":\"assistant\"}\n",
        )
        .unwrap();
        let other = dir.path().join("x.jsonl");
        std::fs::write(
            &other,
            "{\"timestamp\":\"2026-11-12T10:00:00Z\",\"type\":\"session_meta\",\"payload\":{\"cwd\":\"/u/other\"}}\n",
        )
        .unwrap();
        let subagent = dir.path().join("s.jsonl");
        std::fs::write(
            &subagent,
            "{\"timestamp\":\"2026-11-12T10:00:00Z\",\"type\":\"user\",\"cwd\":\"/u/.t3/worktrees/demo/t3-2\",\"message\":{\"content\":\"Act as the test sub-agent\"}}\n",
        )
        .unwrap();
        let mut cache = HashMap::new();
        let session = read_transcript(&claude, &repo, 0, &mut cache).unwrap();
        assert_eq!(session.events.len(), 2);
        assert_eq!(session.prompts.len(), 1);
        assert!(read_transcript(&other, &repo, 0, &mut cache).is_none());
        assert!(read_transcript(&subagent, &repo, 0, &mut cache).is_none());
    }
}
