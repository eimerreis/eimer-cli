//! `eimer product check`: validates update posts (CI gate for the website).

use std::collections::{BTreeMap, BTreeSet};

use anyhow::Result;
use clap::Args;
use serde::Serialize;

use super::changes::read_changelogs;
use super::config::{self, Repo, is_iso_date};
use super::frontmatter::Document;
use super::posts::{self, Post};
use super::template;
use crate::core::error::SilentExit;
use crate::core::ui;

#[derive(Args)]
pub struct CheckArgs {
    /// Treat unpublished drafts as errors
    #[arg(long)]
    strict: bool,
    /// Print machine-readable JSON
    #[arg(long, short = 'j')]
    json: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Finding {
    pub file: String,
    pub message: String,
    pub error: bool,
}

const MEDIA_KINDS: &[&str] = &["image", "video", "demo", "none"];

pub fn run(args: CheckArgs) -> Result<()> {
    let repo = config::load()?;
    let findings = check(&repo, args.strict)?;
    let errors = findings.iter().filter(|f| f.error).count();
    if args.json {
        ui::print_json(&findings)?;
    } else {
        for f in &findings {
            let line = format!("{}: {}", f.file, f.message);
            if f.error {
                ui::print_error(&line, None);
            } else {
                ui::print_warning(&line, None);
            }
        }
        if errors == 0 {
            let count = posts::list(&repo)?.len();
            let noun = if count == 1 {
                "update post"
            } else {
                "update posts"
            };
            ui::print_success(&format!("{count} {noun} OK"));
        }
    }
    if errors > 0 {
        return Err(SilentExit(1).into());
    }
    Ok(())
}

pub fn check(repo: &Repo, strict: bool) -> Result<Vec<Finding>> {
    let posts = posts::list(repo)?;
    let known_ids: BTreeSet<String> = read_changelogs(repo)?.iter().map(|e| e.id()).collect();
    let mut findings = Vec::new();

    if let Some(path) = &repo.config.prompt_template {
        match std::fs::read_to_string(repo.path(path)) {
            Ok(raw) => {
                for name in template::unknown_placeholders(&raw) {
                    findings.push(Finding {
                        file: path.clone(),
                        message: format!("unknown placeholder {{{{{name}}}}}"),
                        error: true,
                    });
                }
            }
            Err(_) => findings.push(Finding {
                file: path.clone(),
                message: "prompt template not found".into(),
                error: true,
            }),
        }
    }

    // base → languages present, and whether any language is published
    let mut languages: BTreeMap<&str, (BTreeSet<&str>, bool)> = BTreeMap::new();
    for post in &posts {
        let file = repo.relative(&post.path);
        let mut push = |message: String, error: bool| {
            findings.push(Finding {
                file: file.clone(),
                message,
                error,
            })
        };
        if post.base.is_empty() {
            push(
                "file name must be <YYYY-MM-DD>-<slug>.<lang>.md".into(),
                true,
            );
            continue;
        }
        if !repo.config.languages.contains(&post.lang) {
            push(format!("language '{}' is not configured", post.lang), true);
        }
        let doc = match &post.doc {
            Ok(doc) => doc,
            Err(err) => {
                push(format!("{err:#}"), true);
                continue;
            }
        };
        for (message, error) in check_post(repo, post, doc, &known_ids) {
            push(message, error);
        }
        let published = doc.str("status") == Some("published");
        if !published {
            push("draft, not published yet".into(), strict);
        }
        let entry = languages.entry(&post.base).or_default();
        entry.0.insert(&post.lang);
        entry.1 |= published;
    }

    for (base, (present, published)) in &languages {
        if !published {
            continue;
        }
        for lang in &repo.config.languages {
            if !present.contains(lang.as_str()) {
                findings.push(Finding {
                    file: format!(
                        "{}/{base}.{lang}.md",
                        repo.config.updates_dir.trim_end_matches('/')
                    ),
                    message: "missing translation of a published post".into(),
                    error: true,
                });
            }
        }
    }
    Ok(findings)
}

fn check_post(
    repo: &Repo,
    post: &Post,
    doc: &Document,
    known_ids: &BTreeSet<String>,
) -> Vec<(String, bool)> {
    let mut out = Vec::new();
    let mut error = |m: String| out.push((m, true));

    match doc.str("title") {
        Some(t) if !t.trim().is_empty() => {}
        _ => error("`title` is required".into()),
    }
    match doc.str("date") {
        Some(d) if is_iso_date(d) => {
            if !post.base.starts_with(d) {
                error(format!("`date` {d} does not match the file name"));
            }
        }
        _ => error("`date` must be YYYY-MM-DD".into()),
    }
    match doc.list("platforms") {
        Some(list) => {
            let known = &repo.config.platforms;
            for p in list
                .iter()
                .filter(|p| !known.is_empty() && !known.contains_key(*p))
            {
                error(format!("unknown platform '{p}'"));
            }
        }
        None => error("`platforms` must be a list, e.g. [iphone, mac]".into()),
    }
    match doc.list("changes") {
        Some([]) | None => error("`changes` must list at least one change id".into()),
        Some(ids) => {
            for id in ids.iter().filter(|id| !known_ids.contains(*id)) {
                error(format!("change '{id}' is not in any configured changelog"));
            }
        }
    }
    let status = doc.str("status").unwrap_or_default();
    if status != "draft" && status != "published" {
        error("`status` must be draft or published".into());
    }
    if status == "published" {
        check_media(repo, doc, &mut error);
    }
    out
}

fn check_media(repo: &Repo, doc: &Document, error: &mut impl FnMut(String)) {
    let Some(media) = doc.map("media") else {
        error("`media` must be a block with `kind` (use `kind: none` for no media)".into());
        return;
    };
    let get = |key: &str| {
        media
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    };
    let kind = get("kind").unwrap_or_default();
    if !MEDIA_KINDS.contains(&kind) {
        error(format!(
            "`media.kind` must be one of {}",
            MEDIA_KINDS.join(", ")
        ));
        return;
    }
    match kind {
        "none" => {}
        "demo" => match get("component") {
            Some(path) if repo.path(path).exists() => {}
            Some(path) => error(format!("`media.component` {path} does not exist")),
            None => error("`media.component` is required for kind demo".into()),
        },
        _ => {
            match get("src") {
                Some(src) => check_media_file(repo, "src", src, error),
                None => error("`media.src` is required".into()),
            }
            if let Some(dark) = get("dark") {
                check_media_file(repo, "dark", dark, error);
            }
            if get("alt").is_none_or(|alt| alt.trim().is_empty()) {
                error("`media.alt` is required".into());
            }
        }
    }
}

fn check_media_file(repo: &Repo, key: &str, url: &str, error: &mut impl FnMut(String)) {
    let prefix = format!("{}/", repo.config.media_url.trim_end_matches('/'));
    let Some(rest) = url.strip_prefix(&prefix) else {
        error(format!("`media.{key}` must start with {prefix}"));
        return;
    };
    let file = repo
        .path(repo.config.media_dir.trim_end_matches('/'))
        .join(rest);
    if !file.is_file() {
        error(format!(
            "`media.{key}` {} does not exist",
            repo.relative(&file)
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn setup(dir: &Path) -> Repo {
        let write = |path: &str, raw: &str| {
            let path = dir.join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, raw).unwrap();
        };
        write(
            "CHANGELOG.md",
            "## 1.0.0\n\n### Minor Changes\n\n- abc1234: Mac planner.\n",
        );
        write("public/updates/mac/planner.png", "png");
        let config = config::parse(
            r#"{
              "products": { "app": { "changelog": "CHANGELOG.md" } },
              "platforms": { "mac": ["**"] },
              "updatesDir": "updates",
              "mediaDir": "public/updates",
              "languages": ["en", "de"]
            }"#,
        )
        .unwrap();
        Repo {
            root: dir.to_path_buf(),
            config,
        }
    }

    const GOOD: &str = "---
title: Mac app
date: 2026-11-12
platforms: [mac]
changes: [abc1234]
media:
  kind: image
  src: /updates/mac/planner.png
  alt: Planner
status: published
---
Body
";

    fn messages(findings: &[Finding]) -> Vec<String> {
        findings.iter().map(|f| f.message.clone()).collect()
    }

    #[test]
    fn valid_posts_pass() {
        let dir = tempfile::tempdir().unwrap();
        let repo = setup(dir.path());
        std::fs::create_dir_all(dir.path().join("updates")).unwrap();
        std::fs::write(dir.path().join("updates/2026-11-12-mac.en.md"), GOOD).unwrap();
        std::fs::write(dir.path().join("updates/2026-11-12-mac.de.md"), GOOD).unwrap();
        assert_eq!(
            messages(&check(&repo, false).unwrap()),
            Vec::<String>::new()
        );
    }

    #[test]
    fn reports_problems() {
        let dir = tempfile::tempdir().unwrap();
        let repo = setup(dir.path());
        std::fs::create_dir_all(dir.path().join("updates")).unwrap();
        let bad = GOOD
            .replace("platforms: [mac]", "platforms: [watch]")
            .replace("[abc1234]", "[fffffff]")
            .replace("planner.png", "missing.png")
            .replace("date: 2026-11-12", "date: 2026-11-13");
        std::fs::write(dir.path().join("updates/2026-11-12-mac.en.md"), bad).unwrap();
        std::fs::write(dir.path().join("updates/notes.md"), "x").unwrap();
        let found = messages(&check(&repo, false).unwrap());
        assert!(found.iter().any(|m| m.contains("unknown platform 'watch'")));
        assert!(found.iter().any(|m| m.contains("change 'fffffff'")));
        assert!(
            found
                .iter()
                .any(|m| m.contains("missing.png does not exist"))
        );
        assert!(
            found
                .iter()
                .any(|m| m.contains("does not match the file name"))
        );
        assert!(found.iter().any(|m| m.contains("file name must be")));
        assert!(found.iter().any(|m| m.contains("missing translation")));
    }

    #[test]
    fn drafts_are_warnings_unless_strict() {
        let dir = tempfile::tempdir().unwrap();
        let repo = setup(dir.path());
        std::fs::create_dir_all(dir.path().join("updates")).unwrap();
        let draft = "---\ntitle: Mac\ndate: 2026-11-12\nplatforms: [mac]\nchanges: [abc1234]\nmedia: auto\nstatus: draft\n---\n";
        std::fs::write(dir.path().join("updates/2026-11-12-mac.en.md"), draft).unwrap();
        let lenient = check(&repo, false).unwrap();
        assert!(lenient.iter().all(|f| !f.error));
        assert!(check(&repo, true).unwrap().iter().any(|f| f.error));
    }
}
