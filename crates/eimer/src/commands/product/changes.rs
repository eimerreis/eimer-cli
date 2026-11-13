//! Changes from the product changelogs that no update post covers yet.

use std::collections::BTreeSet;
use std::path::Path;

use anyhow::{Context, Result};
use serde::Serialize;

use super::changelog::{self, ChangelogEntry};
use super::config::Repo;
use super::glob;
use super::posts;
use crate::core::process::try_run_text;

/// One changelog entry, merged across products that list the same commit.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Change {
    pub id: String,
    pub hash: Option<String>,
    /// `product@version` for every changelog that lists the change.
    pub releases: Vec<String>,
    pub products: Vec<String>,
    pub text: String,
    /// Conventional-commit type and scope of the commit (`feat`, `fix`, ...).
    pub kind: Option<String>,
    pub scope: Option<String>,
    /// Commit date, YYYY-MM-DD.
    pub date: Option<String>,
    pub platforms: Vec<String>,
    /// Why the change starts unchecked, if it does.
    pub internal: Option<String>,
}

impl Change {
    pub fn summary(&self) -> &str {
        self.text.lines().next().unwrap_or_default()
    }
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Collected {
    /// Newest first.
    pub pending: Vec<Change>,
    pub covered: usize,
    pub ignored: usize,
    pub before_since: usize,
}

#[derive(Debug, Clone, Default)]
pub struct CommitInfo {
    pub date: String,
    pub subject: String,
    pub files: Vec<String>,
}

/// Commit lookup, faked in tests.
pub trait Git {
    fn commit(&self, hash: &str) -> Option<CommitInfo>;
}

pub struct SystemGit<'a> {
    pub root: &'a Path,
}

impl Git for SystemGit<'_> {
    fn commit(&self, hash: &str) -> Option<CommitInfo> {
        let root = self.root.to_string_lossy();
        let raw = try_run_text([
            "git",
            "-C",
            &root,
            "show",
            "--no-renames",
            "--name-only",
            "--format=%cs%x1f%s",
            hash,
        ])?;
        let mut lines = raw.lines();
        let (date, subject) = lines.next()?.split_once('\u{1f}')?;
        Some(CommitInfo {
            date: date.to_string(),
            subject: subject.to_string(),
            files: lines
                .filter(|l| !l.trim().is_empty())
                .map(String::from)
                .collect(),
        })
    }
}

/// `feat(mac)!: x` → (`feat`, `mac`).
pub fn conventional_type(subject: &str) -> Option<(String, Option<String>)> {
    let (head, _) = subject.split_once(':')?;
    let head = head.strip_suffix('!').unwrap_or(head);
    let (kind, scope) = match head.split_once('(') {
        Some((kind, rest)) => (kind, Some(rest.strip_suffix(')')?.to_string())),
        None => (head, None),
    };
    (!kind.is_empty() && kind.chars().all(|c| c.is_ascii_lowercase()))
        .then(|| (kind.to_string(), scope))
}

/// Every changelog entry of every configured product, newest first per changelog.
pub fn read_changelogs(repo: &Repo) -> Result<Vec<ChangelogEntry>> {
    let mut entries = Vec::new();
    for (product, source) in &repo.config.products {
        let path = repo.path(&source.changelog);
        let raw = match std::fs::read_to_string(&path) {
            Ok(raw) => raw,
            // A product that never released has no changelog yet.
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => continue,
            Err(err) => {
                return Err(err).with_context(|| format!("Failed to read {}", path.display()));
            }
        };
        entries.extend(changelog::parse(product, &raw));
    }
    Ok(entries)
}

/// Merges entries by id, keeping the first text and collecting releases.
fn merge(entries: Vec<ChangelogEntry>) -> Vec<Change> {
    let mut changes: Vec<Change> = Vec::new();
    for entry in entries {
        let id = entry.id();
        if let Some(change) = changes.iter_mut().find(|c| c.id == id) {
            change.releases.push(entry.release());
            if !change.products.contains(&entry.product) {
                change.products.push(entry.product.clone());
            }
            continue;
        }
        changes.push(Change {
            id,
            hash: entry.hash.clone(),
            releases: vec![entry.release()],
            products: vec![entry.product.clone()],
            text: entry.text,
            kind: None,
            scope: None,
            date: None,
            platforms: Vec::new(),
            internal: None,
        });
    }
    changes
}

pub fn collect(repo: &Repo, git: &dyn Git) -> Result<Collected> {
    let posts = posts::list(repo)?;
    let covered = posts::covered_ids(&posts);
    let ignored: BTreeSet<String> = posts::read_ignored(&repo.ignore_file())?
        .ignored
        .into_iter()
        .map(|c| c.id)
        .collect();

    let mut result = Collected::default();
    for mut change in merge(read_changelogs(repo)?) {
        if covered.contains(&change.id) {
            result.covered += 1;
            continue;
        }
        if ignored.contains(&change.id) {
            result.ignored += 1;
            continue;
        }
        if let Some(info) = change.hash.as_deref().and_then(|h| git.commit(h)) {
            if let Some((kind, scope)) = conventional_type(&info.subject) {
                change.kind = Some(kind);
                change.scope = scope;
            }
            change.platforms = glob::platforms_for(&repo.config.platforms, &info.files);
            change.date = Some(info.date);
        }
        if let (Some(since), Some(date)) = (&repo.config.since, &change.date)
            && date < since
        {
            result.before_since += 1;
            continue;
        }
        change.internal = internal_reason(repo, &change);
        result.pending.push(change);
    }
    // Newest first; changes without a date (no commit found) keep their place at the end.
    result.pending.sort_by(|a, b| {
        b.date
            .is_some()
            .cmp(&a.date.is_some())
            .then(b.date.cmp(&a.date))
    });
    Ok(result)
}

fn internal_reason(repo: &Repo, change: &Change) -> Option<String> {
    if let Some(kind) = &change.kind
        && repo.config.internal_types.contains(kind)
    {
        return Some(format!("{kind} commit"));
    }
    let all_excluded = change
        .products
        .iter()
        .all(|p| repo.config.products.get(p).is_some_and(|s| !s.include));
    all_excluded.then(|| format!("{} only", change.products.join(", ")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    struct FakeGit(HashMap<&'static str, CommitInfo>);

    impl Git for FakeGit {
        fn commit(&self, hash: &str) -> Option<CommitInfo> {
            self.0.get(hash).cloned()
        }
    }

    fn info(date: &str, subject: &str, files: &[&str]) -> CommitInfo {
        CommitInfo {
            date: date.into(),
            subject: subject.into(),
            files: files.iter().map(|f| f.to_string()).collect(),
        }
    }

    fn repo(dir: &Path) -> Repo {
        let write = |path: &str, raw: &str| {
            let path = dir.join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, raw).unwrap();
        };
        write(
            "app/CHANGELOG.md",
            "# app\n\n## 1.2.0\n\n### Minor Changes\n\n- aaaaaaa: Mac planner.\n- bbbbbbb: Shared fix.\n- ccccccc: CI tweak.\n\n## 1.1.0\n\n### Minor Changes\n\n- ddddddd: Old feature.\n",
        );
        write(
            "backend/CHANGELOG.md",
            "# backend\n\n## 2.0.0\n\n### Major Changes\n\n- bbbbbbb: Shared fix.\n- eeeeeee: Backend only.\n",
        );
        write(
            "updates/2026-11-01-old.en.md",
            "---\ntitle: Old\nchanges: [ddddddd]\n---\n",
        );
        write(
            "updates/.ignored.json",
            r#"{ "ignored": [{ "id": "ccccccc", "text": "CI tweak." }] }"#,
        );
        let config = super::super::config::parse(
            r#"{
              "since": "2026-10-01",
              "products": {
                "app": { "changelog": "app/CHANGELOG.md" },
                "backend": { "changelog": "backend/CHANGELOG.md", "include": false }
              },
              "platforms": { "mac": ["apps/ios/**/Mac*/**"], "web": ["web/**"] },
              "updatesDir": "updates",
              "mediaDir": "public/updates"
            }"#,
        )
        .unwrap();
        Repo {
            root: dir.to_path_buf(),
            config,
        }
    }

    #[test]
    fn collects_pending_changes() {
        let dir = tempfile::tempdir().unwrap();
        let repo = repo(dir.path());
        let git = FakeGit(HashMap::from([
            (
                "aaaaaaa",
                info(
                    "2026-11-12",
                    "feat(mac): planner",
                    &["apps/ios/Sources/MacPlanner/A.swift"],
                ),
            ),
            ("bbbbbbb", info("2026-11-10", "fix: shared", &["web/a.ts"])),
            (
                "eeeeeee",
                info("2026-11-11", "feat(api): x", &["backend/a.ts"]),
            ),
        ]));
        let result = collect(&repo, &git).unwrap();
        assert_eq!(result.covered, 1);
        assert_eq!(result.ignored, 1);
        let ids: Vec<&str> = result.pending.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(ids, ["aaaaaaa", "eeeeeee", "bbbbbbb"]);

        let mac = &result.pending[0];
        assert_eq!(mac.kind.as_deref(), Some("feat"));
        assert_eq!(mac.scope.as_deref(), Some("mac"));
        assert_eq!(mac.platforms, ["mac"]);
        assert_eq!(mac.internal, None);

        let shared = &result.pending[2];
        assert_eq!(shared.releases, ["app@1.2.0", "backend@2.0.0"]);
        assert_eq!(shared.internal, None);

        assert_eq!(result.pending[1].internal.as_deref(), Some("backend only"));
    }

    #[test]
    fn skips_changes_before_since() {
        let dir = tempfile::tempdir().unwrap();
        let repo = repo(dir.path());
        let git = FakeGit(HashMap::from([(
            "aaaaaaa",
            info("2026-09-01", "feat: old", &[]),
        )]));
        let result = collect(&repo, &git).unwrap();
        assert_eq!(result.before_since, 1);
        assert!(result.pending.iter().all(|c| c.id != "aaaaaaa"));
    }

    #[test]
    fn parses_conventional_subjects() {
        assert_eq!(
            conventional_type("feat(mac)!: planner"),
            Some(("feat".into(), Some("mac".into())))
        );
        assert_eq!(conventional_type("ci: x"), Some(("ci".into(), None)));
        assert_eq!(conventional_type("Merge branch 'x'"), None);
        assert_eq!(conventional_type("Fix: x"), None);
    }
}
