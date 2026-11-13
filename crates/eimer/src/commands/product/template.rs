//! The prompt handed to a coding agent. Repos override it with `promptTemplate`.

use anyhow::{Context, Result};

use super::changes::Change;
use super::config::Repo;

pub const DEFAULT_TEMPLATE: &str = include_str!("default_prompt.md");

/// Placeholders a template can use, with what they expand to.
pub const PLACEHOLDERS: &[(&str, &str)] = &[
    ("draft", "draft post path, relative to the repo root"),
    ("slug", "post base name, <date>-<slug>"),
    ("title", "title of the draft"),
    ("changes", "selected changes as a bullet list"),
    ("platforms", "platform tags of the selected changes"),
    ("media", "requested media: auto, screenshot, demo or none"),
    (
        "store",
        "yes when store screenshots should be refreshed too",
    ),
    ("languages", "all languages"),
    ("translations", "post paths of the other languages"),
    ("media_dir", "folder for this post's media"),
    ("media_url", "URL prefix of this post's media"),
    (
        "capture",
        "capture commands for the platforms, as a bullet list",
    ),
];

pub struct PromptInput<'a> {
    pub repo: &'a Repo,
    pub draft: &'a str,
    pub slug: &'a str,
    pub title: &'a str,
    pub changes: &'a [Change],
    pub platforms: &'a [String],
    pub media: &'a str,
    pub store: bool,
}

pub fn load(repo: &Repo) -> Result<String> {
    match &repo.config.prompt_template {
        Some(path) => std::fs::read_to_string(repo.path(path))
            .with_context(|| format!("Failed to read prompt template {path}")),
        None => Ok(DEFAULT_TEMPLATE.to_string()),
    }
}

pub fn render(template: &str, input: &PromptInput) -> String {
    let config = &input.repo.config;
    let changes = input
        .changes
        .iter()
        .map(|c| {
            let mut meta = vec![c.releases.join(", ")];
            if let Some(kind) = &c.kind {
                meta.push(match &c.scope {
                    Some(scope) => format!("{kind}({scope})"),
                    None => kind.clone(),
                });
            }
            if !c.platforms.is_empty() {
                meta.push(c.platforms.join(", "));
            }
            let text = indent_continuation(&c.text);
            format!("- `{}` ({}): {text}", c.id, meta.join("; "))
        })
        .collect::<Vec<_>>()
        .join("\n");

    let capture_platforms: Vec<&String> = if input.platforms.is_empty() {
        config.capture.keys().collect()
    } else {
        input
            .platforms
            .iter()
            .filter(|p| config.capture.contains_key(*p))
            .collect()
    };
    let capture = if capture_platforms.is_empty() {
        "- No capture command configured. Find how this repo runs its UI tests.".to_string()
    } else {
        capture_platforms
            .iter()
            .map(|p| format!("- {p}: `{}`", config.capture[*p]))
            .collect::<Vec<_>>()
            .join("\n")
    };

    let primary = input.repo.primary_language();
    let translations = config
        .languages
        .iter()
        .filter(|l| l.as_str() != primary)
        .map(|l| {
            let dir = config.updates_dir.trim_end_matches('/');
            format!("`{dir}/{}.{l}.md`", input.slug)
        })
        .collect::<Vec<_>>();
    let translations = if translations.is_empty() {
        "none".to_string()
    } else {
        translations.join(", ")
    };

    let slug_name = input.slug.get(11..).unwrap_or(input.slug);
    let values = [
        ("draft", input.draft.to_string()),
        ("slug", input.slug.to_string()),
        ("title", input.title.to_string()),
        ("changes", changes),
        (
            "platforms",
            if input.platforms.is_empty() {
                "unknown".into()
            } else {
                input.platforms.join(", ")
            },
        ),
        ("media", input.media.to_string()),
        ("store", if input.store { "yes" } else { "no" }.to_string()),
        ("languages", config.languages.join(", ")),
        ("translations", translations),
        (
            "media_dir",
            format!("{}/{slug_name}", config.media_dir.trim_end_matches('/')),
        ),
        (
            "media_url",
            format!("{}/{slug_name}", config.media_url.trim_end_matches('/')),
        ),
        ("capture", capture),
    ];

    let mut out = template.to_string();
    for (key, value) in values {
        out = out.replace(&format!("{{{{{key}}}}}"), &value);
    }
    out
}

/// Indents every line after the first by two spaces, leaving blank lines empty.
pub fn indent_continuation(text: &str) -> String {
    text.lines()
        .enumerate()
        .map(|(i, l)| {
            if i == 0 || l.is_empty() {
                l.to_string()
            } else {
                format!("  {l}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// `{{name}}` placeholders in `template` that `render` does not know.
pub fn unknown_placeholders(template: &str) -> Vec<String> {
    let mut unknown = Vec::new();
    let mut rest = template;
    while let Some(start) = rest.find("{{") {
        let after = &rest[start + 2..];
        let Some(end) = after.find("}}") else { break };
        let name = after[..end].trim();
        if !PLACEHOLDERS.iter().any(|(k, _)| *k == name) && !unknown.contains(&name.to_string()) {
            unknown.push(name.to_string());
        }
        rest = &after[end + 2..];
    }
    unknown
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn repo() -> Repo {
        let config = super::super::config::parse(
            r#"{
              "products": { "app": { "changelog": "CHANGELOG.md" } },
              "updatesDir": "content/updates",
              "mediaDir": "public/updates",
              "languages": ["en", "de"],
              "capture": { "mac": "fastlane capture_mac test:{test}" }
            }"#,
        )
        .unwrap();
        Repo {
            root: PathBuf::from("/repo"),
            config,
        }
    }

    #[test]
    fn default_template_uses_only_known_placeholders() {
        assert!(unknown_placeholders(DEFAULT_TEMPLATE).is_empty());
        assert_eq!(unknown_placeholders("{{draft}} {{nope}}"), ["nope"]);
    }

    #[test]
    fn renders_values() {
        let repo = repo();
        let change = Change {
            id: "abc1234".into(),
            hash: Some("abc1234".into()),
            releases: vec!["app@1.2.0".into()],
            products: vec!["app".into()],
            text: "Mac planner.\n\nMore.".into(),
            kind: Some("feat".into()),
            scope: Some("mac".into()),
            date: Some("2026-11-12".into()),
            platforms: vec!["mac".into()],
            internal: None,
        };
        let platforms = vec!["mac".to_string()];
        let out = render(
            DEFAULT_TEMPLATE,
            &PromptInput {
                repo: &repo,
                draft: "content/updates/2026-11-12-mac.en.md",
                slug: "2026-11-12-mac",
                title: "Mac",
                changes: &[change],
                platforms: &platforms,
                media: "auto",
                store: false,
            },
        );
        assert!(!out.contains("{{"));
        assert!(out.contains("content/updates/2026-11-12-mac.en.md"));
        assert!(out.contains("`content/updates/2026-11-12-mac.de.md`"));
        assert!(out.contains("public/updates/mac"));
        assert!(out.contains("- `abc1234` (app@1.2.0; feat(mac); mac): Mac planner.\n\n  More."));
        assert!(out.contains("- mac: `fastlane capture_mac test:{test}`"));
    }
}
