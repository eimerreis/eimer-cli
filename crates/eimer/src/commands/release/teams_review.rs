//! Interactive review of the changelog before posting it to Teams (port of `teams-review.ts`).

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};

use super::util::{spawn_detached, which};
use crate::core::process::run_inherit;
use crate::core::prompt;

pub struct ReviewResult {
    pub markdown: String,
    pub should_post: bool,
}

#[derive(Clone, Copy, PartialEq)]
enum Action {
    Post,
    Edit,
    SavePost,
    Cancel,
}

pub fn review_teams_markdown(markdown: &str) -> Result<ReviewResult> {
    let mut current = markdown.to_string();
    print_preview(&current, "Changelog to post to Teams:");

    loop {
        let mut choices = vec![
            (Action::Post, "Post now".to_string()),
            (
                Action::Edit,
                format!("Edit changelog (Open in {})", describe_editor()),
            ),
        ];
        if which("code").is_some() {
            choices.push((
                Action::SavePost,
                "Edit and post on save (Open in VS Code; next save posts automatically)".into(),
            ));
        }
        choices.push((Action::Cancel, "Skip post".into()));

        let labels: Vec<String> = choices.iter().map(|(_, l)| l.clone()).collect();
        let action = choices[prompt::select("Teams webhook review", &labels, 0)?].0;

        match action {
            Action::Post => {
                return Ok(ReviewResult {
                    markdown: current,
                    should_post: true,
                });
            }
            Action::Cancel => {
                return Ok(ReviewResult {
                    markdown: current,
                    should_post: false,
                });
            }
            Action::SavePost => {
                return Ok(ReviewResult {
                    markdown: edit_and_wait_for_save(&current)?,
                    should_post: true,
                });
            }
            Action::Edit => {
                current = edit_in_editor(&current)?;
                print_preview(&current, "Updated changelog to post:");
            }
        }
    }
}

fn edit_in_editor(markdown: &str) -> Result<String> {
    let file = write_editable_file(markdown)?;
    let result = (|| {
        let path = file.display().to_string();
        let mut cmd = resolve_editor_command()?;
        cmd.push(path);
        let code = run_inherit(&cmd)?;
        if code != 0 {
            bail!("Editor exited with code {code}.");
        }
        let updated = read_edited(&file)?;
        if updated.trim().is_empty() {
            bail!("Edited changelog is empty.");
        }
        Ok(updated)
    })();
    let _ = std::fs::remove_file(&file);
    result
}

fn edit_and_wait_for_save(markdown: &str) -> Result<String> {
    let file = write_editable_file(markdown)?;
    let path = file.display().to_string();
    spawn_detached(&["code", "--reuse-window", &path])?;

    println!("Opened {path} in VS Code.");
    println!("Waiting for next save. Save the file in VS Code to post automatically.");
    println!("The review file stays on disk: {path}");

    let mut last_mtime = mtime(&file)?;
    std::thread::sleep(Duration::from_millis(1200));
    loop {
        std::thread::sleep(Duration::from_millis(400));
        let current = mtime(&file)?;
        if current <= last_mtime {
            continue;
        }
        last_mtime = current;
        let updated = read_edited(&file)?;
        if !updated.trim().is_empty() {
            return Ok(updated);
        }
    }
}

fn mtime(path: &Path) -> Result<SystemTime> {
    Ok(std::fs::metadata(path)?.modified()?)
}

/// Writes the markdown to a visible `eimer-release-changelog-*.md` in the cwd.
fn write_editable_file(markdown: &str) -> Result<PathBuf> {
    let file = resolve_editable_file_path()?;
    std::fs::write(&file, format!("{markdown}\n"))
        .with_context(|| format!("Failed to write {}", file.display()))?;
    let written = std::fs::read_to_string(&file)?;
    if written.trim().is_empty() {
        bail!(
            "Editable changelog file was empty after writing: {}",
            file.display()
        );
    }
    Ok(file)
}

fn resolve_editable_file_path() -> Result<PathBuf> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let random = now.subsec_nanos() ^ std::process::id().rotate_left(16);
    let prefix = format!("eimer-release-changelog-{}-{random:08x}", now.as_millis());
    let dir = std::env::current_dir()?;
    for index in 0..100 {
        let suffix = if index == 0 {
            String::new()
        } else {
            format!("-{index}")
        };
        let candidate = dir.join(format!("{prefix}{suffix}.md"));
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    bail!("Could not allocate editable changelog file path.")
}

fn read_edited(path: &Path) -> Result<String> {
    Ok(std::fs::read_to_string(path)?.trim_end().to_string())
}

fn configured_editor() -> Option<String> {
    ["GIT_EDITOR", "VISUAL", "EDITOR"]
        .iter()
        .filter_map(|name| std::env::var(name).ok())
        .map(|v| v.trim().to_string())
        .find(|v| !v.is_empty())
}

/// Editor command without the file argument. A configured editor value is split on
/// whitespace (e.g. `code --wait`) instead of being run through a shell.
fn resolve_editor_command() -> Result<Vec<String>> {
    if let Some(editor) = configured_editor() {
        return Ok(editor.split_whitespace().map(String::from).collect());
    }
    let owned = |items: &[&str]| items.iter().map(|s| s.to_string()).collect();
    if which("code").is_some() {
        return Ok(owned(&["code", "--wait"]));
    }
    if cfg!(target_os = "macos") && which("open").is_some() {
        return Ok(owned(&["open", "-W", "-t"]));
    }
    if which("nano").is_some() {
        return Ok(owned(&["nano"]));
    }
    if which("vi").is_some() {
        return Ok(owned(&["vi"]));
    }
    bail!("No editor found. Set GIT_EDITOR, VISUAL, or EDITOR.")
}

fn describe_editor() -> String {
    if let Some(editor) = configured_editor() {
        return editor;
    }
    if which("code").is_some() {
        "VS Code".into()
    } else if cfg!(target_os = "macos") && which("open").is_some() {
        "default text editor".into()
    } else if which("nano").is_some() {
        "nano".into()
    } else if which("vi").is_some() {
        "vi".into()
    } else {
        "configured editor".into()
    }
}

fn print_preview(markdown: &str, title: &str) {
    println!("\n{title}\n");
    println!("{markdown}");
    println!();
}
