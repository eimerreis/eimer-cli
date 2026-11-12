//! Automatic time entries: `eimer time add --auto` and the git `pre-push` hook.
//!
//! On push, the branch gets one pending entry (`.time/*.md`) holding the focus time
//! measured since the last recorded window; later pushes add to the same entry.
//! The hook commits the entry and stops the push once, so the next push carries it.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::{Args, Subcommand};

use super::activity::{self, RepoIdentity};
use super::pending::{self, Entry};
use super::{duration, repo, suggest};
use crate::core::error::{HintExt, SilentExit};
use crate::core::process::{run_text, try_run_text};
use crate::core::ui;

/// Prompts closer than this count as continuous focus.
pub const DEFAULT_GAP_SECS: i64 = 10 * 60;
const HOOK_MARKER: &str = "# eimer time pre-push hook";
const HOOK_SCRIPT: &str = "#!/bin/sh\n# eimer time pre-push hook\n# Records focus time in .time/ before pushing. Skip once with EIMER_TIME_SKIP=1.\ncommand -v eimer >/dev/null 2>&1 || exit 0\nexec eimer time hook pre-push \"$@\"\n";

#[derive(Args)]
pub struct HookArgs {
    #[command(subcommand)]
    command: HookCommand,
}

#[derive(Subcommand)]
enum HookCommand {
    /// Install the pre-push hook in this clone (shared by all its worktrees)
    Install {
        /// Replace an existing pre-push hook
        #[arg(long)]
        force: bool,
    },
    /// Remove the pre-push hook
    Uninstall,
    /// Run by git before a push (reads the pushed refs from stdin)
    #[command(hide = true)]
    PrePush {
        #[arg(trailing_var_arg = true)]
        _args: Vec<String>,
    },
}

pub fn run(args: HookArgs) -> Result<()> {
    match args.command {
        HookCommand::Install { force } => install(force),
        HookCommand::Uninstall => uninstall(),
        HookCommand::PrePush { .. } => pre_push(),
    }
}

fn hook_path() -> Result<PathBuf> {
    let path = run_text(["git", "rev-parse", "--git-path", "hooks/pre-push"])
        .context("Failed to find the git hooks directory")?;
    let path = PathBuf::from(path.trim());
    Ok(if path.is_absolute() {
        path
    } else {
        std::env::current_dir()?.join(path)
    })
}

pub fn install(force: bool) -> Result<()> {
    let path = hook_path()?;
    if let Ok(existing) = std::fs::read_to_string(&path)
        && !existing.contains(HOOK_MARKER)
        && !force
    {
        return Err(anyhow::anyhow!(
            "{} already exists and is not the eimer time hook",
            path.display()
        ))
        .hint("Add `eimer time hook pre-push \"$@\"` to it, or rerun with --force to replace it.");
    }
    std::fs::create_dir_all(path.parent().expect("hook has a parent"))?;
    std::fs::write(&path, HOOK_SCRIPT)
        .with_context(|| format!("Failed to write {}", path.display()))?;
    make_executable(&path)?;
    ui::print_success(&format!("Installed the pre-push hook ({})", path.display()));
    Ok(())
}

fn uninstall() -> Result<()> {
    let path = hook_path()?;
    match std::fs::read_to_string(&path) {
        Ok(existing) if existing.contains(HOOK_MARKER) => {
            std::fs::remove_file(&path)?;
            ui::print_success("Removed the pre-push hook");
        }
        Ok(_) => ui::print_warning(
            "The pre-push hook is not the eimer time hook; left as is",
            None,
        ),
        Err(_) => ui::print_info("No pre-push hook installed", None),
    }
    Ok(())
}

#[cfg(unix)]
fn make_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
        .context("Failed to make the hook executable")
}

#[cfg(not(unix))]
fn make_executable(_: &Path) -> Result<()> {
    Ok(())
}

/// Never blocks a push because of its own errors: the CI check still catches a
/// missing entry. Stops the push once when it committed an entry.
fn pre_push() -> Result<()> {
    if std::env::var_os("EIMER_TIME_SKIP").is_some() {
        return Ok(());
    }
    let mut stdin = String::new();
    std::io::Read::read_to_string(&mut std::io::stdin(), &mut stdin).ok();
    match pre_push_inner(&stdin) {
        Ok(Some(message)) => {
            ui::print_info(
                &message,
                Some("Push again to include it. Edit the hours in the file if they are off."),
            );
            Err(SilentExit(1).into())
        }
        Ok(None) => Ok(()),
        Err(err) => {
            ui::print_warning(
                &format!("eimer time: no time entry recorded: {err:#}"),
                Some("Run `eimer time add <duration>` yourself; the PR check requires an entry."),
            );
            Ok(())
        }
    }
}

fn pre_push_inner(stdin: &str) -> Result<Option<String>> {
    if repo::read_config()?.is_none() {
        return Ok(None);
    }
    let Some(branch) = try_run_text(["git", "symbolic-ref", "--short", "-q", "HEAD"])
        .map(|b| b.trim().to_string())
        .filter(|b| !b.is_empty())
    else {
        return Ok(None);
    };
    if Some(branch.as_str()) == default_branch().as_deref()
        || !pushes_branch(stdin, &format!("refs/heads/{branch}"))
    {
        return Ok(None);
    }
    record(true)
}

/// Whether the push updates `local_ref` (an empty stdin, e.g. a manual run, counts).
fn pushes_branch(stdin: &str, local_ref: &str) -> bool {
    let lines: Vec<_> = stdin.lines().filter(|l| !l.trim().is_empty()).collect();
    lines.is_empty()
        || lines.iter().any(|line| {
            let parts: Vec<_> = line.split_whitespace().collect();
            parts.first() == Some(&local_ref)
                && parts
                    .get(1)
                    .is_some_and(|sha| sha.chars().any(|c| c != '0'))
        })
}

fn default_branch() -> Option<String> {
    try_run_text([
        "git",
        "symbolic-ref",
        "--short",
        "-q",
        "refs/remotes/origin/HEAD",
    ])
    .and_then(|r| r.trim().strip_prefix("origin/").map(str::to_string))
}

fn merge_base() -> Result<String> {
    for base in ["origin/HEAD", "origin/main", "origin/master"] {
        if let Some(sha) = try_run_text(["git", "merge-base", "HEAD", base]) {
            return Ok(sha.trim().to_string());
        }
    }
    bail!("Failed to find the default branch (origin/HEAD)")
}

/// The pending entry this branch adds, if it already committed one.
fn branch_entry(root: &Path, base: &str) -> Option<PathBuf> {
    let added = try_run_text([
        "git",
        "diff",
        "--name-only",
        "--diff-filter=A",
        base,
        "HEAD",
        "--",
        pending::DIR,
    ])?;
    added
        .lines()
        .find(|p| repo::is_pending_entry(p))
        .map(|p| root.join(p))
        .filter(|p| p.exists())
}

/// Subjects of the branch's commits, oldest first, without time entry commits.
fn commit_subjects(base: &str) -> Vec<String> {
    let log = try_run_text([
        "git",
        "log",
        "--reverse",
        "--format=%s",
        &format!("{base}..HEAD"),
    ])
    .unwrap_or_default();
    log.lines()
        .map(str::trim)
        .filter(|s| !s.is_empty() && !s.starts_with("chore(time)"))
        .map(str::to_string)
        .collect()
}

/// Adds `- subject` lines that are not in the description yet.
fn merge_subjects(description: &str, subjects: &[String]) -> String {
    let mut out = description.trim().to_string();
    for subject in subjects {
        if !out.contains(subject.as_str()) {
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str(&format!("- {subject}"));
        }
    }
    out
}

/// Adds `focus` (seconds) to an entry. Hours follow the measured focus while the
/// user has not edited them; edited hours grow by the new focus only.
fn accumulate(entry: &Entry, focus: i64) -> (f64, i64) {
    let minutes = entry
        .focus_minutes
        .unwrap_or((entry.hours * 60.0).round() as i64)
        + focus / 60;
    let untouched = entry
        .focus_minutes
        .is_some_and(|m| suggest::round_up(m * 60) == entry.hours);
    let hours = if untouched {
        suggest::round_up(minutes * 60)
    } else {
        suggest::round_up((entry.hours * 3600.0).round() as i64 + focus)
    };
    (hours, minutes)
}

/// Records the focus since the last window into the branch's entry (creating it on
/// the first run). Returns a message when the entry changed and was committed.
pub fn record(commit: bool) -> Result<Option<String>> {
    let config = repo::load_config()?;
    if config.projects.len() > 1 {
        return Err(anyhow::anyhow!("This repo bills to several time projects"))
            .hint("Record the entry with `eimer time add <duration> --project <alias>`.");
    }
    let root = repo::root()?;
    let base = merge_base()?;
    let identity = RepoIdentity::current()?;
    let now = chrono::Local::now().timestamp();
    let since = activity::covered_until(&identity)?.unwrap_or_else(super::book::start_of_today);
    let focus = activity::focus_seconds(&identity, since, now, DEFAULT_GAP_SECS)?;
    let subjects = commit_subjects(&base);

    let (path, entry, created) = match branch_entry(&root, &base) {
        Some(path) => {
            let mut entry = pending::read(&path)?;
            let (hours, minutes) = accumulate(&entry, focus);
            // Only a change in hours is worth stopping a push. Smaller focus stays
            // unrecorded (the window is not advanced) until it adds up.
            if hours == entry.hours {
                return Ok(None);
            }
            let description = merge_subjects(&entry.description, &subjects);
            entry.hours = hours;
            entry.focus_minutes = Some(minutes);
            entry.description = description;
            pending::overwrite(&path, &entry)?;
            (path, entry, false)
        }
        None => {
            let entry = Entry {
                hours: suggest::round_up(focus),
                date: chrono::Local::now().format("%Y-%m-%d").to_string(),
                project: None,
                focus_minutes: Some(focus / 60),
                description: merge_subjects("", &subjects),
            };
            let path = pending::write(&root, &entry)?;
            (path, entry, true)
        }
    };
    activity::set_covered_until(&identity, now)?;

    let relative = path.strip_prefix(&root).unwrap_or(&path).to_path_buf();
    let verb = if created { "Recorded" } else { "Updated" };
    let message = format!(
        "{verb} {} ({}m focus) in {}",
        duration::format(entry.hours),
        entry.focus_minutes.unwrap_or_default(),
        relative.display()
    );
    if commit {
        let file = relative.to_string_lossy().to_string();
        run_text(["git", "add", "--", &file]).context("Failed to stage the time entry")?;
        run_text([
            "git",
            "commit",
            "--no-verify",
            "--only",
            "-m",
            &format!("chore(time): record {}", duration::format(entry.hours)),
            "--",
            &file,
        ])
        .context("Failed to commit the time entry")?;
    }
    Ok(Some(message))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(hours: f64, focus_minutes: Option<i64>) -> Entry {
        Entry {
            hours,
            date: "2026-11-12".into(),
            project: None,
            focus_minutes,
            description: String::new(),
        }
    }

    #[test]
    fn accumulates_focus() {
        // untouched: hours follow focus (50m + 20m = 70m → 1.25h)
        assert_eq!(accumulate(&entry(1.0, Some(50)), 20 * 60), (1.25, 70));
        // edited to 2h: grows by the new focus only (2h + 20m → 2.5h)
        assert_eq!(accumulate(&entry(2.0, Some(50)), 20 * 60), (2.5, 70));
        // nothing new
        assert_eq!(accumulate(&entry(1.0, Some(50)), 0), (1.0, 50));
        // hand-written entry without focus
        assert_eq!(accumulate(&entry(1.0, None), 10 * 60), (1.25, 70));
    }

    #[test]
    fn merges_commit_subjects() {
        let subjects = vec!["feat: a".to_string(), "fix: b".to_string()];
        assert_eq!(merge_subjects("", &subjects), "- feat: a\n- fix: b");
        assert_eq!(
            merge_subjects("Hero redesign\n- feat: a", &subjects),
            "Hero redesign\n- feat: a\n- fix: b"
        );
    }

    #[test]
    fn detects_pushed_branch() {
        let sha = "a".repeat(40);
        let zero = "0".repeat(40);
        let line = |local: &str, s: &str| format!("{local} {s} refs/heads/x {zero}\n");
        assert!(pushes_branch("", "refs/heads/x"));
        assert!(pushes_branch(&line("refs/heads/x", &sha), "refs/heads/x"));
        assert!(!pushes_branch(&line("refs/heads/y", &sha), "refs/heads/x"));
        assert!(!pushes_branch(&line("(delete)", &zero), "refs/heads/x"));
    }

    #[test]
    fn hook_script_is_marked() {
        assert!(HOOK_SCRIPT.contains(HOOK_MARKER));
        assert!(HOOK_SCRIPT.starts_with("#!/bin/sh\n"));
    }
}
