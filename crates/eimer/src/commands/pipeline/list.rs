//! `pipeline list [pattern...]`: pipeline definitions, optionally filtered by glob.

use anyhow::{Context, Result};
use clap::Args;
use serde_json::{Value, json};

use super::common::{az_json, str_at};
use crate::core::process::run_json;
use crate::core::ui;

#[derive(Args)]
pub struct ListArgs {
    /// Glob filters on pipeline name (same as --filter)
    #[arg(value_name = "PATTERN")]
    patterns: Vec<String>,
    /// Glob filter on pipeline name (repeatable)
    #[arg(long, short = 'f')]
    filter: Vec<String>,
    /// Maximum number of pipelines to fetch
    #[arg(long, short = 'n', default_value_t = 1000, value_parser = clap::value_parser!(u64).range(1..))]
    top: u64,
    /// Print machine-readable JSON
    #[arg(long, short = 'j')]
    json: bool,
}

pub fn run(args: ListArgs) -> Result<()> {
    list(args).context("Failed to list pipelines")
}

fn list(args: ListArgs) -> Result<()> {
    let patterns: Vec<String> = args
        .filter
        .iter()
        .chain(&args.patterns)
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty())
        .collect();

    let top = args.top.to_string();
    let all: Vec<Value> = ui::spin_unless(args.json, "Loading pipelines", || {
        run_json(az_json(&["pipelines", "list", "--top", &top]))
    })?;

    let name_of = |p: &Value| str_at(p, &["name"]).unwrap_or_default().to_string();
    let mut filtered: Vec<&Value> = all
        .iter()
        .filter(|p| matches_any(&name_of(p), &patterns))
        .collect();
    filtered.sort_by_cached_key(|p| name_of(p).to_lowercase());

    if args.json {
        return ui::print_json(&json!({
            "totalFetched": all.len(),
            "count": filtered.len(),
            "filters": patterns,
            "items": filtered,
        }));
    }

    if filtered.is_empty() {
        if patterns.is_empty() {
            ui::print_info("No pipelines found.", None);
        } else {
            ui::print_info(
                &format!("No pipelines match: {}.", patterns.join(", ")),
                Some("Try --filter '*name*' or increase --top."),
            );
        }
        return Ok(());
    }

    let rows = filtered.iter().map(|p| {
        let revision = match p.get("revision") {
            Some(Value::Number(n)) => n.to_string(),
            Some(Value::String(s)) => s.clone(),
            _ => "-".into(),
        };
        vec![
            format!("#{}", p.get("id").and_then(Value::as_u64).unwrap_or(0)),
            str_at(p, &["name"]).unwrap_or("Unnamed").to_string(),
            str_at(p, &["path"]).unwrap_or("\\").to_string(),
            str_at(p, &["queueStatus"]).unwrap_or("-").to_string(),
            revision,
        ]
    });
    println!(
        "{}",
        ui::render_table(["Pipeline", "Name", "Path", "Queue", "Rev"], rows)
    );
    Ok(())
}

fn matches_any(name: &str, patterns: &[String]) -> bool {
    patterns.is_empty() || patterns.iter().any(|p| glob_match(p, name))
}

/// Case-insensitive glob over the whole name: `*` = any run, `?` = one character.
fn glob_match(pattern: &str, name: &str) -> bool {
    let pattern: Vec<char> = pattern.to_lowercase().chars().collect();
    let name: Vec<char> = name.to_lowercase().chars().collect();
    let (mut p, mut n) = (0, 0);
    let mut backtrack: Option<(usize, usize)> = None;
    while n < name.len() {
        match pattern.get(p) {
            Some('*') => {
                backtrack = Some((p, n));
                p += 1;
            }
            Some(&c) if c == '?' || c == name[n] => {
                p += 1;
                n += 1;
            }
            _ => match backtrack {
                Some((bp, bn)) => {
                    p = bp + 1;
                    n = bn + 1;
                    backtrack = Some((bp, bn + 1));
                }
                None => return false,
            },
        }
    }
    pattern[p..].iter().all(|c| *c == '*')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn globs() {
        assert!(glob_match("*deploy*", "App-Deploy-Prod"));
        assert!(glob_match("app-?", "APP-1"));
        assert!(!glob_match("app-?", "app-12"));
        assert!(glob_match("app", "App"));
        assert!(!glob_match("app", "app-ci"));
        assert!(glob_match("a*b*c", "aXXbYYc"));
        assert!(!glob_match("a*b*c", "aXXbYY"));
        assert!(glob_match("*", ""));
        assert!(glob_match("a.b(c)", "A.B(C)"));
        assert!(!glob_match("a.b", "axb"));
    }

    #[test]
    fn no_patterns_match_everything() {
        assert!(matches_any("anything", &[]));
        assert!(matches_any("x-ci", &["*-cd".into(), "*-ci".into()]));
    }
}
