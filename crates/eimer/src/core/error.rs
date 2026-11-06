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

/// Prints an error (with hint if present) and returns the process exit code.
pub fn report(err: &anyhow::Error) -> i32 {
    if let Some(SilentExit(code)) = err.downcast_ref::<SilentExit>() {
        return *code;
    }
    match err.chain().find_map(|e| e.downcast_ref::<Hinted>()) {
        Some(hinted) => {
            // Context added on top of the hinted error is still part of the message.
            super::ui::print_error(&format!("{err:#}"), Some(&hinted.hint));
        }
        None => super::ui::print_error(&format!("{err:#}"), None),
    }
    1
}
