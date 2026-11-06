//! Error helpers: attach user-facing hints and request specific exit codes.

use std::fmt;

/// An error carrying a hint that is printed below the error message.
#[derive(Debug)]
pub struct Hinted {
    pub hint: String,
    pub source: anyhow::Error,
}

impl fmt::Display for Hinted {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:#}", self.source)
    }
}

impl std::error::Error for Hinted {}

/// Exit with `code` without printing anything (the command already reported).
#[derive(Debug)]
pub struct SilentExit(pub i32);

impl fmt::Display for SilentExit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "exit {}", self.0)
    }
}

impl std::error::Error for SilentExit {}

pub trait HintExt<T> {
    /// Attaches a hint to the error, e.g. `.hint("Try --all")`.
    fn hint(self, hint: impl Into<String>) -> anyhow::Result<T>;
}

impl<T, E: Into<anyhow::Error>> HintExt<T> for Result<T, E> {
    fn hint(self, hint: impl Into<String>) -> anyhow::Result<T> {
        self.map_err(|err| {
            Hinted {
                hint: hint.into(),
                source: err.into(),
            }
            .into()
        })
    }
}

/// The most specific (innermost) hint attached anywhere in the error chain.
fn find_hint(err: &anyhow::Error) -> Option<&str> {
    err.chain()
        .find_map(|e| e.downcast_ref::<Hinted>())
        .map(|hinted| find_hint(&hinted.source).unwrap_or(&hinted.hint))
}

/// Prints an error (with hint if present) and returns the process exit code.
pub fn report(err: &anyhow::Error) -> i32 {
    if let Some(SilentExit(code)) = err.downcast_ref::<SilentExit>() {
        return *code;
    }
    super::ui::print_error(&format!("{err:#}"), find_hint(err));
    1
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::{Context, anyhow};

    #[test]
    fn innermost_hint_wins_and_message_keeps_context() {
        let inner: anyhow::Result<()> = Err(anyhow!("boom")).hint("specific");
        let err = inner.context("Failed to do X").hint("general").unwrap_err();
        assert_eq!(find_hint(&err), Some("specific"));
        assert_eq!(format!("{err:#}"), "Failed to do X: boom");
    }
}
