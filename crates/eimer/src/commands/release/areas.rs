//! Area filters for changelogs (port of `areas.ts` and `filterCommitsByArea`).

use std::collections::{BTreeMap, HashSet};

use anyhow::{Result, bail};

use super::changelog_format::CommitInfo;
use super::config::AreaConfig;
use super::prod_stage::normalize_text;

fn area(include: &[&str], exclude: &[&str], keywords: &[&str]) -> AreaConfig {
    let owned = |items: &[&str]| items.iter().map(|s| s.to_string()).collect();
    AreaConfig {
        include_scopes: owned(include),
        exclude_scopes: owned(exclude),
        exclude_keywords: owned(keywords),
    }
}

pub fn default_area_configs() -> BTreeMap<String, AreaConfig> {
    BTreeMap::from([
        (
            "frontend".to_string(),
            area(
                &[
                    "frontend",
                    "front-end",
                    "ui",
                    "web",
                    "client",
                    "ux",
                    "design-system",
                ],
                &[
                    "backend",
                    "api",
                    "service",
                    "infra",
                    "ops",
                    "db",
                    "migration",
                ],
                &[
                    "nuget",
                    "csproj",
                    "dotnet",
                    "entityframework",
                    "sql migration",
                ],
            ),
        ),
        (
            "backend".to_string(),
            area(
                &["backend", "api", "service", "db", "worker", "integration"],
                &["frontend", "front-end", "ui", "web", "client", "ux"],
                &["storybook", "css", "tailwind", "nuxt", "react", "nextjs"],
            ),
        ),
        (
            "infra".to_string(),
            area(
                &[
                    "infra",
                    "ops",
                    "devops",
                    "pipeline",
                    "ci",
                    "cd",
                    "k8s",
                    "terraform",
                ],
                &["frontend", "front-end", "ui", "backend", "api"],
                &["feature flag", "ui test", "playwright"],
            ),
        ),
    ])
}

/// Built-in areas, overridden/extended by configured ones (names are lowercased).
pub fn merge_area_configs(
    custom: Option<&BTreeMap<String, AreaConfig>>,
) -> BTreeMap<String, AreaConfig> {
    let mut merged = default_area_configs();
    for (name, config) in custom.into_iter().flatten() {
        let name = name.trim().to_lowercase();
        if !name.is_empty() {
            merged.insert(name, config.clone());
        }
    }
    merged
}

pub struct AreaFilterResult {
    pub included: Vec<CommitInfo>,
    pub manual_review: Vec<CommitInfo>,
    pub area_name: String,
}

pub fn filter_commits_by_area(
    commits: Vec<CommitInfo>,
    area_name: Option<&str>,
    area_configs: &BTreeMap<String, AreaConfig>,
) -> Result<AreaFilterResult> {
    let raw_area = area_name.unwrap_or_default();
    let normalized = normalize_text(raw_area);
    if normalized.is_empty() {
        return Ok(AreaFilterResult {
            included: commits,
            manual_review: Vec::new(),
            area_name: String::new(),
        });
    }

    let Some(config) = area_configs.get(&normalized) else {
        let available: Vec<&str> = area_configs.keys().map(String::as_str).collect();
        bail!(
            "Unknown area '{raw_area}'. Available areas: {}",
            available.join(", ")
        );
    };

    let include: HashSet<String> = config
        .include_scopes
        .iter()
        .map(|s| normalize_text(s))
        .collect();
    let exclude: HashSet<String> = config
        .exclude_scopes
        .iter()
        .map(|s| normalize_text(s))
        .collect();
    let keywords: Vec<String> = config
        .exclude_keywords
        .iter()
        .map(|s| normalize_text(s))
        .collect();

    let mut included = Vec::new();
    let mut manual_review = Vec::new();
    for commit in commits {
        let subject = normalize_text(&commit.subject);
        let scope = normalize_text(&commit.conventional.scope);
        if keywords
            .iter()
            .any(|k| !k.is_empty() && subject.contains(k.as_str()))
        {
            continue;
        }
        if !scope.is_empty() && exclude.contains(&scope) {
            continue;
        }
        if !scope.is_empty() && include.contains(&scope) {
            included.push(commit);
            continue;
        }
        manual_review.push(commit);
    }

    Ok(AreaFilterResult {
        included,
        manual_review,
        area_name: normalized,
    })
}

#[cfg(test)]
mod tests {
    use super::super::changelog_format::commit_from_message;
    use super::*;

    fn commit_from(hash: &str, message: &str) -> CommitInfo {
        commit_from_message(hash, message, "https://dev.azure.com/org/proj", "repo-id")
    }

    #[test]
    fn filters_by_area() {
        let commits = vec![
            commit_from("1", "feat(ui): button"),
            commit_from("2", "feat(api): endpoint"),
            commit_from("3", "fix: bump nuget package"),
            commit_from("4", "chore: something"),
            commit_from("5", "feat(Web): page"),
        ];
        let areas = default_area_configs();
        let r = filter_commits_by_area(commits.clone(), Some(" Frontend "), &areas).unwrap();
        assert_eq!(r.area_name, "frontend");
        let subjects = |v: &[CommitInfo]| v.iter().map(|c| c.subject.clone()).collect::<Vec<_>>();
        assert_eq!(
            subjects(&r.included),
            ["feat(ui): button", "feat(Web): page"]
        );
        assert_eq!(subjects(&r.manual_review), ["chore: something"]);

        let all = filter_commits_by_area(commits.clone(), None, &areas).unwrap();
        assert_eq!(all.included.len(), 5);
        assert!(all.manual_review.is_empty());

        let err = filter_commits_by_area(commits, Some("mobile"), &areas)
            .err()
            .unwrap();
        assert_eq!(
            err.to_string(),
            "Unknown area 'mobile'. Available areas: backend, frontend, infra"
        );
    }
}
