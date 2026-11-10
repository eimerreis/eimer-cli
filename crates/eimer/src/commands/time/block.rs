//! The `Time:` line in a PR body. Keep the accepted syntax in sync with the
//! `time-check` workflow in eimerreis/freelance-ops.

use super::duration;

#[derive(Debug, Clone, PartialEq)]
pub enum TimeEntry {
    Booked {
        hours: f64,
        record_id: u64,
        project_alias: Option<String>,
    },
    None {
        reason: String,
    },
}

pub const BLOCK_START: &str = "<!-- tt:time -->";
pub const BLOCK_END: &str = "<!-- /tt:time -->";
const PREFIX: &str = "Time:";

pub fn format_line(entry: &TimeEntry) -> String {
    match entry {
        TimeEntry::None { reason } => format!("{PREFIX} none ({reason})"),
        TimeEntry::Booked {
            hours,
            record_id,
            project_alias,
        } => {
            let alias = project_alias
                .as_ref()
                .map(|a| format!(" @{a}"))
                .unwrap_or_default();
            format!(
                "{PREFIX} {}{alias} → time#{record_id}",
                duration::format(*hours)
            )
        }
    }
}

/// Parses `Time: 1.5h [@alias] → time#123` (also `->`) or `Time: none (reason)`.
pub fn parse_line(line: &str) -> Option<TimeEntry> {
    let rest = line.trim().strip_prefix(PREFIX)?.trim();

    if let Some(reason) = rest.strip_prefix("none") {
        let reason = reason.trim().strip_prefix('(')?.strip_suffix(')')?.trim();
        return (!reason.is_empty()).then(|| TimeEntry::None {
            reason: reason.to_string(),
        });
    }

    let (left, right) = rest.split_once('→').or_else(|| rest.split_once("->"))?;
    let record_id = right.trim().strip_prefix("time#")?;
    if record_id.is_empty() || !record_id.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }

    let mut parts = left.split_whitespace();
    let hours = parts.next()?.strip_suffix('h')?;
    let valid_hours = !hours.is_empty()
        && hours.chars().all(|c| c.is_ascii_digit() || c == '.')
        && !hours.starts_with('.')
        && !hours.ends_with('.')
        && hours.matches('.').count() <= 1;
    if !valid_hours {
        return None;
    }
    let project_alias = match parts.next() {
        None => None,
        Some(alias) => {
            let alias = alias.strip_prefix('@')?;
            let valid = !alias.is_empty()
                && alias
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
            if !valid {
                return None;
            }
            Some(alias.to_string())
        }
    };
    if parts.next().is_some() {
        return None;
    }

    Some(TimeEntry::Booked {
        hours: hours.parse().ok()?,
        record_id: record_id.parse().ok()?,
        project_alias,
    })
}

/// The first `Time:` line of the body and its entry (None when invalid).
pub fn find(body: &str) -> Option<(String, Option<TimeEntry>)> {
    body.lines()
        .find(|line| line.starts_with(PREFIX))
        .map(|line| (line.to_string(), parse_line(line)))
}

/// Writes the entry into the body: replaces the block, else the first `Time:` line, else appends.
pub fn upsert(body: &str, entry: &TimeEntry) -> String {
    let block = format!("{BLOCK_START}\n{}\n{BLOCK_END}", format_line(entry));

    if let (Some(start), Some(end)) = (body.find(BLOCK_START), body.find(BLOCK_END))
        && end > start
    {
        return format!(
            "{}{block}{}",
            &body[..start],
            &body[end + BLOCK_END.len()..]
        );
    }
    if let Some((line, _)) = find(body) {
        return body.replacen(&line, &block, 1);
    }
    let trimmed = body.trim_end();
    let separator = if trimmed.is_empty() { "" } else { "\n\n" };
    format!("{trimmed}{separator}## Time\n\n{block}\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn booked(hours: f64, record_id: u64, alias: Option<&str>) -> TimeEntry {
        TimeEntry::Booked {
            hours,
            record_id,
            project_alias: alias.map(String::from),
        }
    }

    fn none(reason: &str) -> TimeEntry {
        TimeEntry::None {
            reason: reason.into(),
        }
    }

    #[test]
    fn round_trips_entries() {
        let entry = booked(1.5, 42, Some("web"));
        assert_eq!(format_line(&entry), "Time: 1.5h @web → time#42");
        assert_eq!(parse_line(&format_line(&entry)), Some(entry));
        assert_eq!(format_line(&none("refactor")), "Time: none (refactor)");
        assert_eq!(parse_line("Time: none (refactor)"), Some(none("refactor")));
    }

    #[test]
    fn accepts_ascii_arrow_and_rejects_placeholders() {
        assert_eq!(parse_line("Time: 2h -> time#7"), Some(booked(2.0, 7, None)));
        for line in [
            "Time: pending (run `eimer time book`)",
            "Time: 2h",
            "Time: none ()",
            "Time: 2h → time#",
            "Time: 2h web → time#1",
            "Time: .5h → time#1",
        ] {
            assert_eq!(parse_line(line), None, "{line}");
        }
    }

    #[test]
    fn finds_first_time_line() {
        assert_eq!(
            find("intro\nTime: none (x)\n"),
            Some(("Time: none (x)".into(), Some(none("x"))))
        );
        assert_eq!(find("Time: pending"), Some(("Time: pending".into(), None)));
        assert_eq!(find("nothing"), None);
    }

    #[test]
    fn upserts_block_line_or_appends() {
        let entry = none("x");
        let template = "## Time\n\n<!-- tt:time -->\nTime: pending\n<!-- /tt:time -->\n\nfooter";
        assert_eq!(
            upsert(template, &entry),
            "## Time\n\n<!-- tt:time -->\nTime: none (x)\n<!-- /tt:time -->\n\nfooter"
        );
        assert_eq!(
            upsert("a\nTime: pending\nb", &entry),
            "a\n<!-- tt:time -->\nTime: none (x)\n<!-- /tt:time -->\nb"
        );
        assert_eq!(
            upsert("summary", &entry),
            "summary\n\n## Time\n\n<!-- tt:time -->\nTime: none (x)\n<!-- /tt:time -->\n"
        );
    }
}
