//! Subprocess helpers (az, git, gh, ...). All external tools are invoked directly, never via a shell.

use std::io::Write;
use std::process::{Command, Stdio};

use anyhow::{Context, Result, anyhow, bail};
use serde::de::DeserializeOwned;

fn display(cmd: &[String]) -> String {
    cmd.join(" ")
}

fn to_vec<I, S>(cmd: I) -> Vec<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    cmd.into_iter().map(|s| s.as_ref().to_string()).collect()
}

fn command(cmd: &[String]) -> Result<Command> {
    let (program, args) = cmd.split_first().ok_or_else(|| anyhow!("empty command"))?;
    let mut command = Command::new(resolve_program(program));
    command.args(args);
    Ok(command)
}

/// On Windows, `az` is a `.cmd` shim that `Command` does not find without the extension.
fn resolve_program(program: &str) -> String {
    if cfg!(windows) && program == "az" {
        "az.cmd".to_string()
    } else {
        program.to_string()
    }
}

/// Runs a command, returns stdout. Fails with stderr (or stdout) on a non-zero exit.
pub fn run_text<I, S>(cmd: I) -> Result<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let cmd = to_vec(cmd);
    let output = command(&cmd)?
        .stdin(Stdio::null())
        .output()
        .with_context(|| format!("Failed to start '{}'. Is it installed and on PATH?", cmd[0]))?;

    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let detail = if stderr.trim().is_empty() {
            stdout.trim().to_string()
        } else {
            stderr.trim().to_string()
        };
        bail!("Command failed ({}): {detail}", display(&cmd));
    }
    Ok(stdout)
}

/// Runs a command and parses stdout as JSON.
pub fn run_json<T, I, S>(cmd: I) -> Result<T>
where
    T: DeserializeOwned,
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let cmd = to_vec(cmd);
    let raw = run_text(&cmd)?;
    serde_json::from_str(&raw)
        .with_context(|| format!("Failed to parse JSON from '{}'", display(&cmd)))
}

/// Like [`run_text`] but returns `None` instead of failing.
pub fn try_run_text<I, S>(cmd: I) -> Option<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    run_text(cmd).ok()
}

/// Runs a command with inherited stdio (interactive tools, streaming output). Returns the exit code.
pub fn run_inherit<I, S>(cmd: I) -> Result<i32>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let cmd = to_vec(cmd);
    let status = command(&cmd)?
        .status()
        .with_context(|| format!("Failed to start '{}'. Is it installed and on PATH?", cmd[0]))?;
    Ok(status.code().unwrap_or(1))
}

/// Runs a command, writing `input` to its stdin.
pub fn run_with_stdin<I, S>(cmd: I, input: &str) -> Result<()>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let cmd = to_vec(cmd);
    let mut child = command(&cmd)?
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("Failed to start '{}'", cmd[0]))?;
    child
        .stdin
        .take()
        .ok_or_else(|| anyhow!("no stdin"))?
        .write_all(input.as_bytes())?;
    let output = child.wait_with_output()?;
    if !output.status.success() {
        bail!(
            "Command failed ({}): {}",
            display(&cmd),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(())
}

/// Opens a URL in the default browser.
pub fn open_url(url: &str) -> Result<()> {
    let cmd: Vec<&str> = if cfg!(target_os = "macos") {
        vec!["open", url]
    } else if cfg!(windows) {
        vec!["cmd", "/C", "start", "", url]
    } else {
        vec!["xdg-open", url]
    };
    run_text(cmd).map(|_| ()).context("Failed to open browser")
}

/// Where [`copy_to_clipboard`] put the text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Copied {
    /// A clipboard tool (pbcopy, clip, wl-copy, xclip, xsel) took it.
    Tool,
    /// Sent to the terminal as an OSC 52 escape; the terminal sets the clipboard of the
    /// machine it runs on (also over SSH). Terminals without OSC 52 support ignore it.
    Terminal,
}

/// Copies text to the system clipboard using the platform tool (pbcopy, clip, wl-copy,
/// xclip or xsel). Without one (headless server, SSH session) it falls back to OSC 52
/// when stderr is a terminal.
pub fn copy_to_clipboard(text: &str) -> Result<Copied> {
    let candidates: Vec<Vec<&str>> = if cfg!(target_os = "macos") {
        vec![vec!["pbcopy"]]
    } else if cfg!(windows) {
        vec![vec!["clip"]]
    } else {
        vec![
            vec!["wl-copy"],
            vec!["xclip", "-selection", "clipboard"],
            vec!["xsel", "--clipboard", "--input"],
        ]
    };

    let mut last_error = None;
    for candidate in &candidates {
        match run_with_stdin(candidate, text) {
            Ok(()) => return Ok(Copied::Tool),
            Err(err) if is_not_found(&err) => {}
            Err(err) => last_error = Some(err),
        }
    }
    if let Some(err) = last_error {
        return Err(err).context("Failed to copy to clipboard");
    }
    let mut stderr = std::io::stderr();
    if !std::io::IsTerminal::is_terminal(&stderr) {
        return Err(NoClipboard.into());
    }
    let tmux = std::env::var_os("TMUX").is_some();
    stderr
        .write_all(osc52(text, tmux).as_bytes())
        .and_then(|()| stderr.flush())
        .context("Failed to copy to clipboard")?;
    Ok(Copied::Terminal)
}

/// OSC 52 "set clipboard" sequence; inside tmux wrapped in a passthrough sequence.
fn osc52(text: &str, tmux: bool) -> String {
    let sequence = format!("\x1b]52;c;{}\x07", base64(text.as_bytes()));
    if tmux {
        format!("\x1bPtmux;{}\x1b\\", sequence.replace('\x1b', "\x1b\x1b"))
    } else {
        sequence
    }
}

fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |acc, (i, b)| acc | (u32::from(*b) << (16 - 8 * i)));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(ALPHABET[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// No clipboard tool is installed (headless machine, SSH session).
#[derive(Debug)]
pub struct NoClipboard;

impl std::fmt::Display for NoClipboard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let tools = if cfg!(target_os = "macos") {
            "pbcopy"
        } else if cfg!(windows) {
            "clip"
        } else {
            "wl-copy, xclip or xsel"
        };
        write!(
            f,
            "No clipboard tool found and no terminal to copy through (install {tools})"
        )
    }
}

impl std::error::Error for NoClipboard {}

fn is_not_found(err: &anyhow::Error) -> bool {
    err.chain().any(|e| {
        e.downcast_ref::<std::io::Error>()
            .is_some_and(|io| io.kind() == std::io::ErrorKind::NotFound)
    })
}

#[cfg(test)]
mod clipboard_tests {
    use super::*;

    #[test]
    fn encodes_base64() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64("Größe".as_bytes()), "R3LDtsOfZQ==");
    }

    #[test]
    fn builds_osc52() {
        assert_eq!(osc52("hi", false), "\x1b]52;c;aGk=\x07");
        assert_eq!(osc52("hi", true), "\x1bPtmux;\x1b\x1b]52;c;aGk=\x07\x1b\\");
    }
}
