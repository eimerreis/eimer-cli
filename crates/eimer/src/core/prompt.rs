//! Interactive prompts. Fail fast (instead of hanging) when stdin is not a terminal.

use std::io::IsTerminal;

use anyhow::{Result, bail};
use dialoguer::{Confirm, Input, Select, theme::ColorfulTheme};

pub fn is_interactive() -> bool {
    std::io::stdin().is_terminal() && std::io::stderr().is_terminal()
}

/// Prompts read keys in raw mode, so Ctrl-C arrives as an "interrupted" read error rather
/// than SIGINT. Treat it as a cancel: restore the cursor, end the line, exit 130.
fn finish<T>(result: dialoguer::Result<T>) -> Result<T> {
    match result {
        Ok(value) => Ok(value),
        Err(dialoguer::Error::IO(err)) if err.kind() == std::io::ErrorKind::Interrupted => {
            exit_cancelled()
        }
        Err(err) => Err(err.into()),
    }
}

/// Cancel (Ctrl-C): show the cursor again (selection prompts hide it), end the line, exit 130.
pub fn exit_cancelled() -> ! {
    use std::io::Write;
    let mut stderr = std::io::stderr();
    if stderr.is_terminal() {
        let _ = writeln!(stderr, "\x1b[?25h");
        let _ = stderr.flush();
    }
    std::process::exit(130);
}

/// Text prompt. `validate` returns an error message for invalid input.
pub fn text(
    message: &str,
    placeholder: Option<&str>,
    validate: impl Fn(&str) -> Option<String>,
) -> Result<String> {
    if !is_interactive() {
        bail!("{message} is required (no interactive terminal to prompt)");
    }
    let theme = ColorfulTheme::default();
    let mut input = Input::<String>::with_theme(&theme).with_prompt(message);
    if let Some(placeholder) = placeholder.filter(|p| !p.is_empty()) {
        input = input.default(placeholder.to_string()).show_default(true);
    }
    let value = finish(
        input
            .validate_with(|value: &String| match validate(value) {
                Some(err) => Err(err),
                None => Ok(()),
            })
            .interact_text(),
    )?;
    Ok(value.trim().to_string())
}

/// Text prompt where empty input is allowed and returns `fallback`.
pub fn text_or(
    message: &str,
    fallback: &str,
    validate: impl Fn(&str) -> Option<String>,
) -> Result<String> {
    if !is_interactive() {
        bail!("{message} is required (no interactive terminal to prompt)");
    }
    let theme = ColorfulTheme::default();
    let mut input = Input::<String>::with_theme(&theme)
        .with_prompt(message)
        .allow_empty(true);
    if !fallback.is_empty() {
        input = input.default(fallback.to_string()).show_default(true);
    }
    let value = finish(
        input
            .validate_with(|value: &String| match validate(value) {
                Some(err) => Err(err),
                None => Ok(()),
            })
            .interact_text(),
    )?;
    let value = value.trim();
    Ok(if value.is_empty() { fallback } else { value }.to_string())
}

/// Prompts for a positive integer ID.
pub fn id(message: &str) -> Result<u64> {
    let raw = text(message, None, |v| match v.trim().parse::<u64>() {
        Ok(n) if n > 0 => None,
        _ => Some(format!("{message} must be a positive integer")),
    })?;
    Ok(raw.parse()?)
}

/// Selection prompt; returns the index of the chosen item.
pub fn select(message: &str, items: &[String], default: usize) -> Result<usize> {
    if !is_interactive() {
        bail!("{message}: selection required (no interactive terminal to prompt)");
    }
    finish(
        Select::with_theme(&ColorfulTheme::default())
            .with_prompt(message)
            .items(items)
            .default(default)
            .interact(),
    )
}

/// Yes/no confirmation. Callers must handle the non-interactive case themselves.
pub fn confirm(message: &str, default: bool) -> Result<bool> {
    if !is_interactive() {
        bail!("{message}: confirmation required (no interactive terminal to prompt)");
    }
    finish(
        Confirm::with_theme(&ColorfulTheme::default())
            .with_prompt(message)
            .default(default)
            .interact(),
    )
}
