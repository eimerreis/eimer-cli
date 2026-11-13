//! Parser for changelogs written by `@changesets/cli/changelog`:
//!
//! ```md
//! ## 1.2.0
//! ### Minor Changes
//! - abc1234: First paragraph.
//!
//!   Second paragraph.
//! ```

use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangelogEntry {
    pub product: String,
    pub version: String,
    /// `major`, `minor` or `patch`, from the `### ... Changes` heading.
    pub bump: Option<String>,
    pub hash: Option<String>,
    pub text: String,
}

impl ChangelogEntry {
    /// Stable id: the commit hash, or a hash of the text for entries without one.
    pub fn id(&self) -> String {
        match &self.hash {
            Some(hash) => hash.clone(),
            None => text_id(&self.text),
        }
    }

    pub fn release(&self) -> String {
        format!("{}@{}", self.product, self.version)
    }
}

pub fn text_id(text: &str) -> String {
    let digest = Sha256::digest(text.trim().as_bytes());
    format!("t{}", &hex::encode(digest)[..7])
}

/// Entries newest first, as they appear in the file. Dependency-only bumps are skipped.
pub fn parse(product: &str, raw: &str) -> Vec<ChangelogEntry> {
    let mut entries = Vec::new();
    let mut version: Option<String> = None;
    let mut bump: Option<String> = None;
    let mut current: Option<Vec<String>> = None;

    let mut flush =
        |lines: Option<Vec<String>>, version: &Option<String>, bump: &Option<String>| {
            let (Some(lines), Some(version)) = (lines, version) else {
                return;
            };
            let body = lines.join("\n").trim().to_string();
            if body.is_empty() || body.starts_with("Updated dependencies") {
                return;
            }
            let (hash, text) = split_hash(&body);
            entries.push(ChangelogEntry {
                product: product.to_string(),
                version: version.clone(),
                bump: bump.clone(),
                hash,
                text,
            });
        };

    for line in raw.lines() {
        if let Some(heading) = line.strip_prefix("## ") {
            flush(current.take(), &version, &bump);
            version = Some(heading.trim().to_string());
            bump = None;
        } else if let Some(heading) = line.strip_prefix("### ") {
            flush(current.take(), &version, &bump);
            bump = heading
                .split_whitespace()
                .next()
                .map(|word| word.to_lowercase());
        } else if line.starts_with("# ") {
            flush(current.take(), &version, &bump);
        } else if let Some(item) = line.strip_prefix("- ") {
            flush(current.take(), &version, &bump);
            current = Some(vec![item.to_string()]);
        } else if let Some(lines) = current.as_mut() {
            lines.push(line.strip_prefix("  ").unwrap_or(line).to_string());
        }
    }
    flush(current.take(), &version, &bump);
    entries
}

fn split_hash(body: &str) -> (Option<String>, String) {
    if let Some((head, rest)) = body.split_once(": ")
        && (7..=40).contains(&head.len())
        && head.chars().all(|c| c.is_ascii_hexdigit())
    {
        return (Some(head.to_string()), rest.trim().to_string());
    }
    (None, body.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "# volleyteam-app

## 1.38.3

### Patch Changes

- cd4a817: Fix the Mac build.

  Redirect the stable Mac download URL.

- Updated dependencies [abc1234]
  - @volleyball/domain@1.0.1

## 1.38.0

### Minor Changes

- ef71e4e: Version products independently.
- Entry without a hash.
";

    #[test]
    fn parses_entries_with_paragraphs() {
        let entries = parse("volleyteam-app", SAMPLE);
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].hash.as_deref(), Some("cd4a817"));
        assert_eq!(entries[0].version, "1.38.3");
        assert_eq!(entries[0].bump.as_deref(), Some("patch"));
        assert_eq!(
            entries[0].text,
            "Fix the Mac build.\n\nRedirect the stable Mac download URL."
        );
        assert_eq!(entries[1].release(), "volleyteam-app@1.38.0");
        assert_eq!(entries[1].bump.as_deref(), Some("minor"));
    }

    #[test]
    fn entries_without_hash_get_text_id() {
        let entries = parse("app", SAMPLE);
        assert_eq!(entries[2].hash, None);
        assert!(entries[2].id().starts_with('t'));
        assert_eq!(entries[2].id(), text_id("Entry without a hash."));
    }
}
