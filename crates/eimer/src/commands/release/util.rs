//! Small helpers the core crate does not provide (yet).

use std::io::IsTerminal;
use std::path::PathBuf;

use anyhow::{Result, bail};

use crate::core::prompt;

/// Text prompt where empty input falls back to `fallback`
/// (matches Bunli's `prompt.text(..., { fallbackValue })`).
pub fn text_with_fallback(
    message: &str,
    fallback: &str,
    validate: impl Fn(&str) -> Option<String>,
) -> Result<String> {
    prompt::text_or(message, fallback, validate)
}

/// stdin and stdout are terminals (the TS `isInteractiveTerminal`).
pub fn is_interactive_terminal() -> bool {
    std::io::stdin().is_terminal() && std::io::stdout().is_terminal()
}

/// Finds an executable on PATH.
pub fn which(program: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    let exts: Vec<String> = if cfg!(windows) {
        std::env::var("PATHEXT")
            .unwrap_or_else(|_| ".EXE;.CMD;.BAT".into())
            .split(';')
            .map(|e| e.to_lowercase())
            .chain([String::new()])
            .collect()
    } else {
        vec![String::new()]
    };
    std::env::split_paths(&path).find_map(|dir| {
        exts.iter().find_map(|ext| {
            let candidate = dir.join(format!("{program}{ext}"));
            is_executable(&candidate).then_some(candidate)
        })
    })
}

#[cfg(unix)]
fn is_executable(path: &std::path::Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.metadata()
        .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn is_executable(path: &std::path::Path) -> bool {
    path.is_file()
}

/// Starts a program without waiting for it (stdin/stdout ignored, stderr inherited).
pub fn spawn_detached(cmd: &[&str]) -> Result<()> {
    let Some((program, args)) = cmd.split_first() else {
        bail!("empty command");
    };
    std::process::Command::new(program)
        .args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|err| anyhow::anyhow!("Failed to start '{program}': {err}"))
}
