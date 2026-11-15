//! `eimer time suggest` against real git repos and fake agent transcripts.

use std::path::{Path, PathBuf};
use std::process::Command;

struct Fixture {
    _dir: tempfile::TempDir,
    home: PathBuf,
    repo: PathBuf,
    scratch: PathBuf,
}

/// A home with a repo and a scratch folder (not a repo) next to it.
fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    let home = root.join("home");
    let repo = root.join("repo");
    let scratch = root.join("scratch");
    for d in [&home, &repo, &scratch] {
        std::fs::create_dir_all(d).unwrap();
    }
    let status = Command::new("git")
        .args(["init", "-q"])
        .current_dir(&repo)
        .status()
        .unwrap();
    assert!(status.success());
    Fixture {
        _dir: dir,
        home,
        repo,
        scratch,
    }
}

fn minutes_ago(minutes: i64) -> String {
    (chrono::Utc::now() - chrono::Duration::minutes(minutes)).to_rfc3339()
}

/// A Claude Code transcript started in `cwd` with two prompts 5 minutes apart: 7 focus minutes.
fn transcript(home: &Path, name: &str, cwd: &Path) {
    let dir = home.join(".claude/projects").join(name);
    std::fs::create_dir_all(&dir).unwrap();
    let line = |minutes: i64| {
        format!(
            "{{\"timestamp\":\"{}\",\"type\":\"user\",\"cwd\":\"{}\",\"message\":{{\"content\":\"go on\"}}}}\n",
            minutes_ago(minutes),
            cwd.display()
        )
    };
    std::fs::write(dir.join("s.jsonl"), line(30) + &line(25)).unwrap();
}

fn suggest(f: &Fixture, envs: &[(&str, &Path)], args: &[&str]) -> serde_json::Value {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_eimer"));
    cmd.current_dir(&f.repo)
        .env("HOME", &f.home)
        .env_remove("GIT_DIR")
        .args(["time", "suggest", "--json", "--since", &minutes_ago(60)])
        .args(args);
    for (key, value) in envs {
        cmd.env(key, value);
    }
    let output = cmd.output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn ignores_sessions_outside_the_repo_inside_a_git_hook() {
    let f = fixture();
    transcript(&f.home, "scratch", &f.scratch);
    let git_dir = f.repo.join(".git");

    // Git exports GIT_DIR to hooks; the pre-push hook runs suggest's logic with it set.
    let json = suggest(&f, &[("GIT_DIR", &git_dir)], &[]);

    assert_eq!(json["focus_minutes"], 0);
    assert_eq!(json["prompts"], 0);
}

#[test]
fn counts_sessions_in_the_repo() {
    let f = fixture();
    transcript(&f.home, "repo", &f.repo);

    let json = suggest(&f, &[], &[]);

    assert_eq!(json["focus_minutes"], 7);
    assert_eq!(json["prompts"], 2);
}

#[test]
fn counts_an_included_scratch_folder() {
    let f = fixture();
    transcript(&f.home, "scratch", &f.scratch);
    let scratch = f.scratch.display().to_string();

    let without = suggest(&f, &[], &[]);
    let with = suggest(&f, &[], &["--include", &scratch]);

    assert_eq!(without["focus_minutes"], 0);
    assert_eq!(with["focus_minutes"], 7);
    assert_eq!(with["prompts"], 2);
}
