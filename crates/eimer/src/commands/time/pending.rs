//! Pending time entries (`.time/*.md`, committed with the work, like changesets) and
//! the booking ledger (`TIME.md`, appended by `eimer time book`).
//!
//! Entry file:
//! ```text
//! ---
//! hours: 1.5
//! date: 2026-11-12
//! project: web        # only when .time.json lists several projects
//! ---
//! What was done (optional)
//! ```

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use serde::Serialize;

use super::duration;

pub const DIR: &str = ".time";
pub const LEDGER: &str = "TIME.md";

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Entry {
    pub hours: f64,
    pub date: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    pub description: String,
}

pub fn format(entry: &Entry) -> String {
    let mut out = format!(
        "---\nhours: {}\ndate: {}\n",
        trim_number(entry.hours),
        entry.date
    );
    if let Some(project) = &entry.project {
        out.push_str(&format!("project: {project}\n"));
    }
    out.push_str("---\n");
    if !entry.description.trim().is_empty() {
        out.push_str(entry.description.trim());
        out.push('\n');
    }
    out
}

pub fn parse(raw: &str) -> Result<Entry> {
    let rest = raw
        .trim_start()
        .strip_prefix("---")
        .ok_or_else(|| anyhow!("missing front matter"))?;
    let (front, body) = rest
        .split_once("\n---")
        .ok_or_else(|| anyhow!("unterminated front matter"))?;
    let (mut hours, mut date, mut project) = (None, None, None);
    for line in front.lines().map(str::trim).filter(|l| !l.is_empty()) {
        let (key, value) = line
            .split_once(':')
            .ok_or_else(|| anyhow!("invalid line \"{line}\""))?;
        let value = value.split(" #").next().unwrap_or_default().trim();
        match key.trim() {
            "hours" => hours = Some(duration::parse(value)?),
            "date" => {
                chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d")
                    .map_err(|_| anyhow!("invalid date \"{value}\""))?;
                date = Some(value.to_string());
            }
            "project" => project = Some(value.to_string()).filter(|p| !p.is_empty()),
            other => bail!("unknown key \"{other}\""),
        }
    }
    Ok(Entry {
        hours: hours.ok_or_else(|| anyhow!("missing hours"))?,
        date: date.ok_or_else(|| anyhow!("missing date"))?,
        project,
        description: body.trim().to_string(),
    })
}

/// "1.50" → "1.5", "2.00" → "2".
fn trim_number(hours: f64) -> String {
    let s = format!("{hours:.2}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

pub struct PendingFile {
    pub path: PathBuf,
    pub entry: Entry,
}

/// All pending entries of the repo at `root`, oldest first.
pub fn list(root: &Path) -> Result<Vec<PendingFile>> {
    let dir = root.join(DIR);
    let entries = match std::fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
        Err(err) => return Err(err).context(format!("Failed to read {DIR}/")),
    };
    let mut files = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|e| e != "md")
            || path.file_name().is_some_and(|n| n == "README.md")
        {
            continue;
        }
        let raw = std::fs::read_to_string(&path)
            .with_context(|| format!("Failed to read {}", path.display()))?;
        let entry = parse(&raw).with_context(|| format!("Invalid entry {}", path.display()))?;
        files.push(PendingFile { path, entry });
    }
    files.sort_by(|a, b| (&a.entry.date, &a.path).cmp(&(&b.entry.date, &b.path)));
    Ok(files)
}

/// Writes a new entry as `.time/<date>-<random>.md` and returns its path.
pub fn write(root: &Path, entry: &Entry) -> Result<PathBuf> {
    let dir = root.join(DIR);
    std::fs::create_dir_all(&dir).with_context(|| format!("Failed to create {DIR}/"))?;
    let mut bytes = [0u8; 3];
    getrandom::fill(&mut bytes).map_err(|e| anyhow!("Failed to get random bytes: {e}"))?;
    let path = dir.join(format!("{}-{}.md", entry.date, hex::encode(bytes)));
    std::fs::write(&path, format(entry))
        .with_context(|| format!("Failed to write {}", path.display()))?;
    Ok(path)
}

/// One booked time record, as listed in the ledger.
#[derive(Debug, Clone, Serialize)]
pub struct Booking {
    pub date: String,
    pub project: String,
    pub hours: f64,
    pub record_id: u64,
    pub description: String,
}

const LEDGER_HEADER: &str = "# Time\n\nTime booked to codeunity time, appended by `eimer time book`.\nPending entries live in `.time/` until they are booked.\n";

/// Appends a booking section for `booked_on` to the ledger text.
pub fn append_ledger(existing: Option<&str>, booked_on: &str, bookings: &[Booking]) -> String {
    let mut out = existing
        .map(|e| e.trim_end().to_string())
        .unwrap_or_else(|| LEDGER_HEADER.trim_end().to_string());
    let total: f64 = bookings.iter().map(|b| b.hours).sum();
    out.push_str(&format!(
        "\n\n## Booked {booked_on} ({})\n\n| Date | Project | Hours | Record | Description |\n| --- | --- | --- | --- | --- |\n",
        duration::format(total)
    ));
    for b in bookings {
        out.push_str(&format!(
            "| {} | {} | {} | time#{} | {} |\n",
            b.date,
            b.project,
            duration::format(b.hours),
            b.record_id,
            b.description.replace('|', "\\|").replace('\n', " ")
        ));
    }
    out
}

/// The last booking heading of the ledger, e.g. "Booked 2026-11-12 (4.5h)".
pub fn last_booking(ledger: &str) -> Option<String> {
    ledger
        .lines()
        .rev()
        .find_map(|l| l.strip_prefix("## "))
        .map(str::to_string)
}

/// Merges entry descriptions into one record description, keeping order, dropping repeats.
pub fn summarize(prefix: &str, descriptions: &[&str]) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for d in descriptions
        .iter()
        .map(|d| d.trim())
        .filter(|d| !d.is_empty())
    {
        if !parts.contains(&d) {
            parts.push(d);
        }
    }
    if parts.is_empty() {
        format!("{prefix}: development")
    } else {
        format!("{prefix}: {}", parts.join("; "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(project: Option<&str>) -> Entry {
        Entry {
            hours: 1.5,
            date: "2026-11-12".into(),
            project: project.map(Into::into),
            description: "Ledger for time entries".into(),
        }
    }

    #[test]
    fn round_trips_entries() {
        for e in [entry(None), entry(Some("web"))] {
            assert_eq!(parse(&format(&e)).unwrap(), e);
        }
        assert_eq!(
            format(&entry(None)),
            "---\nhours: 1.5\ndate: 2026-11-12\n---\nLedger for time entries\n"
        );
    }

    #[test]
    fn parses_hand_written_entries() {
        let e = parse("---\nhours: 45m\ndate: 2026-11-01 # monday\n---\n").unwrap();
        assert_eq!(e.hours, 0.75);
        assert_eq!(e.date, "2026-11-01");
        assert_eq!(e.description, "");
        assert!(parse("---\nhours: 1\n---\n").is_err());
        assert!(parse("hours: 1").is_err());
        assert!(parse("---\nhours: 1\ndate: 2026-11-01\ncolor: red\n---\n").is_err());
    }

    #[test]
    fn writes_and_lists_entries() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), &entry(None)).unwrap();
        std::fs::write(dir.path().join(DIR).join("README.md"), "docs").unwrap();
        let files = list(dir.path()).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].entry, entry(None));
    }

    #[test]
    fn appends_ledger_sections() {
        let booking = Booking {
            date: "2026-11-10".into(),
            project: "Lauterbad".into(),
            hours: 1.5,
            record_id: 7,
            description: "a | b".into(),
        };
        let first = append_ledger(None, "2026-11-12", std::slice::from_ref(&booking));
        assert!(first.starts_with("# Time\n"));
        assert!(first.contains("| 2026-11-10 | Lauterbad | 1.5h | time#7 | a \\| b |"));
        let second = append_ledger(Some(&first), "2026-11-20", &[booking]);
        assert_eq!(last_booking(&second).unwrap(), "Booked 2026-11-20 (1.5h)");
    }

    #[test]
    fn summarizes_descriptions() {
        assert_eq!(summarize("repo", &["a", " ", "b", "a"]), "repo: a; b");
        assert_eq!(summarize("repo", &[]), "repo: development");
    }
}
