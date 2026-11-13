//! Update posts on disk: `<updatesDir>/<date>-<slug>.<lang>.md`, plus the ignore file.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use super::config::{Repo, is_iso_date};
use super::frontmatter::{self, Document};

#[derive(Debug)]
pub struct Post {
    pub path: PathBuf,
    /// `<date>-<slug>`, shared by every language of the post.
    pub base: String,
    pub lang: String,
    pub doc: Result<Document>,
}

/// Splits `2026-11-12-mac-app.en.md` into (`2026-11-12-mac-app`, `en`).
pub fn split_file_name(name: &str) -> Option<(String, String)> {
    let stem = name.strip_suffix(".md")?;
    let (base, lang) = stem.rsplit_once('.')?;
    let valid_lang =
        (2..=5).contains(&lang.len()) && lang.chars().all(|c| c.is_ascii_lowercase() || c == '-');
    let valid_base = base.len() > 11
        && is_iso_date(&base[..10])
        && base.as_bytes()[10] == b'-'
        && is_slug(&base[11..]);
    (valid_lang && valid_base).then(|| (base.to_string(), lang.to_string()))
}

pub fn is_slug(value: &str) -> bool {
    !value.is_empty()
        && !value.starts_with('-')
        && !value.ends_with('-')
        && value
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// Markdown files in the updates folder. Files with a bad name come back with `base` empty.
pub fn list(repo: &Repo) -> Result<Vec<Post>> {
    let dir = repo.updates_dir();
    let read = match std::fs::read_dir(&dir) {
        Ok(read) => read,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(err) => {
            return Err(err).with_context(|| format!("Failed to read {}", dir.display()));
        }
    };
    let mut posts = Vec::new();
    for entry in read {
        let path = entry?.path();
        let name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        if !name.ends_with(".md") || name.starts_with('.') || name.eq_ignore_ascii_case("README.md")
        {
            continue;
        }
        let (base, lang) = split_file_name(&name).unwrap_or_default();
        let doc = std::fs::read_to_string(&path)
            .with_context(|| format!("Failed to read {}", path.display()))
            .and_then(|raw| frontmatter::parse(&raw));
        posts.push(Post {
            path,
            base,
            lang,
            doc,
        });
    }
    posts.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(posts)
}

/// Change ids listed in any post's `changes`, drafts included.
pub fn covered_ids(posts: &[Post]) -> BTreeSet<String> {
    posts
        .iter()
        .filter_map(|p| p.doc.as_ref().ok())
        .filter_map(|doc| doc.list("changes"))
        .flatten()
        .cloned()
        .collect()
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct IgnoreFile {
    #[serde(default)]
    pub ignored: Vec<IgnoredChange>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IgnoredChange {
    pub id: String,
    pub text: String,
}

pub fn read_ignored(path: &Path) -> Result<IgnoreFile> {
    match std::fs::read_to_string(path) {
        Ok(raw) => serde_json::from_str(&raw)
            .with_context(|| format!("Failed to parse {}", path.display())),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(IgnoreFile::default()),
        Err(err) => Err(err).with_context(|| format!("Failed to read {}", path.display())),
    }
}

pub fn add_ignored(path: &Path, changes: Vec<IgnoredChange>) -> Result<()> {
    let mut file = read_ignored(path)?;
    let known: BTreeSet<String> = file.ignored.iter().map(|c| c.id.clone()).collect();
    file.ignored
        .extend(changes.into_iter().filter(|c| !known.contains(&c.id)));
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)
            .with_context(|| format!("Failed to create {}", dir.display()))?;
    }
    let raw = serde_json::to_string_pretty(&file)? + "\n";
    std::fs::write(path, raw).with_context(|| format!("Failed to write {}", path.display()))
}

/// Lowercase ASCII slug, max 48 chars; German umlauts are transliterated.
pub fn slugify(value: &str) -> String {
    let mut out = String::new();
    for c in value.to_lowercase().chars() {
        match c {
            'ä' => out.push_str("ae"),
            'ö' => out.push_str("oe"),
            'ü' => out.push_str("ue"),
            'ß' => out.push_str("ss"),
            c if c.is_ascii_alphanumeric() => out.push(c),
            _ => {
                if !out.ends_with('-') && !out.is_empty() {
                    out.push('-');
                }
            }
        }
    }
    let mut slug: String = out.trim_matches('-').chars().take(48).collect();
    while slug.ends_with('-') {
        slug.pop();
    }
    if slug.is_empty() {
        "update".into()
    } else {
        slug
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_valid_names() {
        assert_eq!(
            split_file_name("2026-11-12-mac-app.en.md"),
            Some(("2026-11-12-mac-app".into(), "en".into()))
        );
        assert_eq!(split_file_name("2026-11-12-mac-app.md"), None);
        assert_eq!(split_file_name("mac-app.en.md"), None);
        assert_eq!(split_file_name("2026-11-12-Mac_App.en.md"), None);
    }

    #[test]
    fn slugs() {
        assert_eq!(slugify("VolleyTeam for Mac!"), "volleyteam-for-mac");
        assert_eq!(slugify("Größere Übersicht"), "groessere-uebersicht");
        assert_eq!(slugify("  ---  "), "update");
    }

    #[test]
    fn ignore_file_dedupes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x/.ignored.json");
        let change = |id: &str| IgnoredChange {
            id: id.into(),
            text: "t".into(),
        };
        add_ignored(&path, vec![change("a"), change("b")]).unwrap();
        add_ignored(&path, vec![change("b"), change("c")]).unwrap();
        let ids: Vec<String> = read_ignored(&path)
            .unwrap()
            .ignored
            .into_iter()
            .map(|c| c.id)
            .collect();
        assert_eq!(ids, ["a", "b", "c"]);
    }
}
