//! Human-readable terminal output: colors, status symbols, tables, spinners.
//! Mirrors the conventions of the former `@scripts/ui` package (gh-style output).

use std::io::IsTerminal;
use std::sync::OnceLock;
use std::time::Duration;

use comfy_table::{ContentArrangement, Table};
use indicatif::{ProgressBar, ProgressStyle};
use owo_colors::OwoColorize;

fn colors_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var_os("NO_COLOR").is_none()
            && (std::io::stdout().is_terminal() || std::env::var_os("FORCE_COLOR").is_some())
    })
}

fn paint(value: &str, f: impl Fn(&str) -> String) -> String {
    if colors_enabled() {
        f(value)
    } else {
        value.to_string()
    }
}

pub fn success(value: &str) -> String {
    paint(value, |v| v.green().to_string())
}
pub fn error(value: &str) -> String {
    paint(value, |v| v.red().to_string())
}
pub fn warning(value: &str) -> String {
    paint(value, |v| v.yellow().to_string())
}
pub fn info(value: &str) -> String {
    paint(value, |v| v.cyan().to_string())
}
pub fn dim(value: &str) -> String {
    paint(value, |v| v.dimmed().to_string())
}
pub fn bold(value: &str) -> String {
    paint(value, |v| v.bold().to_string())
}

/// Status symbols, already colored.
pub mod symbols {
    use super::*;
    pub fn arrow() -> String {
        dim(">")
    }
    pub fn fail() -> String {
        error("X")
    }
    pub fn info() -> String {
        super::info("i")
    }
    pub fn neutral() -> String {
        dim("-")
    }
    pub fn ok() -> String {
        success("+")
    }
    pub fn pending() -> String {
        super::warning("*")
    }
    pub fn queued() -> String {
        dim("*")
    }
    pub fn running() -> String {
        super::info("*")
    }
    pub fn warning() -> String {
        super::warning("!")
    }
}

fn hint_line(hint: &str) -> String {
    dim(&format!("Hint: {hint}"))
}

pub fn print_success(message: &str) {
    println!("{} {message}", symbols::ok());
}

pub fn print_info(message: &str, hint: Option<&str>) {
    println!("{} {message}", symbols::info());
    if let Some(hint) = hint.filter(|h| !h.is_empty()) {
        println!("{}", hint_line(hint));
    }
}

pub fn print_warning(message: &str, hint: Option<&str>) {
    println!("{} {message}", symbols::warning());
    if let Some(hint) = hint.filter(|h| !h.is_empty()) {
        println!("{}", hint_line(hint));
    }
}

/// Errors go to stderr.
pub fn print_error(message: &str, hint: Option<&str>) {
    eprintln!("{} {message}", symbols::fail());
    if let Some(hint) = hint.filter(|h| !h.is_empty()) {
        eprintln!("{}", hint_line(hint));
    }
}

/// Bordered ASCII table that wraps to the terminal width.
pub fn render_table<H, R, C>(headers: H, rows: R) -> String
where
    H: IntoIterator,
    H::Item: AsRef<str>,
    R: IntoIterator<Item = Vec<C>>,
    C: Into<String>,
{
    let mut table = Table::new();
    table.set_content_arrangement(ContentArrangement::Dynamic);
    if let Some((terminal_size::Width(width), _)) = terminal_size::terminal_size() {
        table.set_width(width);
    }
    table.set_header(
        headers
            .into_iter()
            .map(|h| bold(h.as_ref()))
            .collect::<Vec<_>>(),
    );
    for row in rows {
        table.add_row(row.into_iter().map(Into::into).collect::<Vec<String>>());
    }
    table.to_string()
}

/// Runs `action` while showing a spinner on stderr (only when stderr is a TTY).
/// The spinner is cleared afterwards; callers print their own result/error.
pub fn with_spinner<T, E>(label: &str, action: impl FnOnce() -> Result<T, E>) -> Result<T, E> {
    if !std::io::stderr().is_terminal() {
        return action();
    }

    let spinner = ProgressBar::new_spinner();
    spinner.set_style(
        ProgressStyle::with_template("{spinner:.cyan} {msg}")
            .unwrap_or_else(|_| ProgressStyle::default_spinner()),
    );
    spinner.set_message(label.to_string());
    spinner.enable_steady_tick(Duration::from_millis(80));
    let result = action();
    spinner.finish_and_clear();
    result
}

/// Like [`with_spinner`] but skipped entirely when `quiet` (e.g. `--json`).
pub fn spin_unless<T, E>(
    quiet: bool,
    label: &str,
    action: impl FnOnce() -> Result<T, E>,
) -> Result<T, E> {
    if quiet {
        action()
    } else {
        with_spinner(label, action)
    }
}

/// OSC 8 hyperlink when stdout is a TTY, plain text otherwise.
pub fn terminal_link(text: &str, url: &str) -> String {
    if url.is_empty() || !std::io::stdout().is_terminal() {
        return text.to_string();
    }
    format!("\u{1b}]8;;{url}\u{7}{text}\u{1b}]8;;\u{7}")
}

/// "just now", "5m ago", "3h ago", "2d ago", "4mo ago", "1y ago". Empty for missing/invalid input.
pub fn relative_time(value: Option<&str>) -> String {
    let Some(value) = value.filter(|v| !v.is_empty()) else {
        return String::new();
    };
    let Ok(parsed) = chrono::DateTime::parse_from_rfc3339(value) else {
        return value.to_string();
    };
    relative_seconds((chrono::Utc::now() - parsed.with_timezone(&chrono::Utc)).num_seconds())
}

fn relative_seconds(delta: i64) -> String {
    let delta = delta.max(0);
    if delta < 60 {
        return "just now".into();
    }
    let minutes = delta / 60;
    if minutes < 60 {
        return format!("{minutes}m ago");
    }
    let hours = minutes / 60;
    if hours < 24 {
        return format!("{hours}h ago");
    }
    let days = hours / 24;
    if days < 30 {
        return format!("{days}d ago");
    }
    let months = days / 30;
    if months < 12 {
        return format!("{months}mo ago");
    }
    format!("{}y ago", months / 12)
}

/// Pretty-prints a serializable value as JSON to stdout.
pub fn print_json<T: serde::Serialize + ?Sized>(value: &T) -> anyhow::Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_buckets() {
        assert_eq!(relative_seconds(-5), "just now");
        assert_eq!(relative_seconds(59), "just now");
        assert_eq!(relative_seconds(60 * 5), "5m ago");
        assert_eq!(relative_seconds(3600 * 3), "3h ago");
        assert_eq!(relative_seconds(86400 * 2), "2d ago");
        assert_eq!(relative_seconds(86400 * 65), "2mo ago");
        assert_eq!(relative_seconds(86400 * 400), "1y ago");
    }

    #[test]
    fn invalid_dates_pass_through() {
        assert_eq!(relative_time(None), "");
        assert_eq!(relative_time(Some("nope")), "nope");
    }
}
