//! Interactive prompts. Fail fast (instead of hanging) when stdin is not a terminal.

use std::io::IsTerminal;

use anyhow::{Result, bail};
use dialoguer::{Input, Select, theme::ColorfulTheme};

pub fn is_interactive() -> bool {
    std::io::stdin().is_terminal() && std::io::stderr().is_terminal()
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
    let value = input
        .validate_with(|value: &String| match validate(value) {
            Some(err) => Err(err),
            None => Ok(()),
        })
        .interact_text()?;
    Ok(value.trim().to_string())
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
    Ok(Select::with_theme(&ColorfulTheme::default())
        .with_prompt(message)
        .items(items)
        .default(default)
        .interact()?)
}
