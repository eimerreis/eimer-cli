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

/// Event streams (one per transcript file) of this repo since `since` (unix seconds).
pub fn collect(repo: &RepoIdentity, since: i64) -> Result<Vec<Vec<i64>>> {
    let home = crate::core::config::home_dir()?;
    let mut files = Vec::new();
    for root in [home.join(".claude/projects"), home.join(".codex/sessions")] {
        jsonl_files(&root, since, &mut files);
    }
    let mut cache = HashMap::new();
    let mut streams = Vec::new();
    for file in files {
        if let Some(events) = read_transcript(&file, repo, since, &mut cache) {
            streams.push(events);
        }
    }
    Ok(streams)
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
struct Line {
    timestamp: Option<String>,
    /// Claude Code: on most lines.
    cwd: Option<String>,
    /// Codex: `session_meta` / `turn_context` payloads.
    payload: Option<Payload>,
}

#[derive(Deserialize)]
struct Payload {
    cwd: Option<String>,
}

/// Timestamps of a transcript that belongs to `repo`; `None` for other repos.
fn read_transcript(
    path: &Path,
    repo: &RepoIdentity,
    since: i64,
    cache: &mut HashMap<PathBuf, bool>,
) -> Option<Vec<i64>> {
    let reader = BufReader::new(std::fs::File::open(path).ok()?);
    let mut matched = None;
    let mut events = Vec::new();
    for raw in reader.lines() {
        let Ok(raw) = raw else { break };
        let Ok(line) = serde_json::from_str::<Line>(&raw) else {
            continue;
        };
        if matched.is_none()
            && let Some(cwd) = line.cwd.or_else(|| line.payload.and_then(|p| p.cwd))
        {
            let cwd = PathBuf::from(cwd.trim_start_matches("file://"));
            if !repo.contains(&cwd, cache) {
                return None;
            }
            matched = Some(true);
        }
        if let Some(ts) = line.timestamp.as_deref().and_then(parse_timestamp)
            && ts >= since
        {
            events.push(ts);
        }
    }
    matched.map(|_| events)
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
             {\"timestamp\":\"2026-11-12T10:00:00Z\",\"cwd\":\"/u/.t3/worktrees/demo/t3-1\"}\n\
             {\"timestamp\":\"2026-11-12T10:05:00Z\"}\n",
        )
        .unwrap();
        let codex = dir.path().join("x.jsonl");
        std::fs::write(
            &codex,
            "{\"timestamp\":\"2026-11-12T10:00:00Z\",\"type\":\"session_meta\",\"payload\":{\"cwd\":\"/u/other\"}}\n",
        )
        .unwrap();
        let mut cache = HashMap::new();
        let events = read_transcript(&claude, &repo, 0, &mut cache).unwrap();
        assert_eq!(events.len(), 2);
        assert!(read_transcript(&codex, &repo, 0, &mut cache).is_none());
    }
}
