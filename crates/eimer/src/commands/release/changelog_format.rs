//! Commit parsing, grouping, area filtering and markdown rendering for changelogs
//! (port of `changelog-format.ts` and the pure helpers of `changelog.ts`).

use anyhow::{Result, bail};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConventionalCommit {
    #[serde(rename = "type")]
    pub commit_type: String,
    pub scope: String,
    pub description: String,
    pub raw: String,
    pub is_conventional: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitInfo {
    pub hash: String,
    pub short_hash: String,
    pub subject: String,
    pub conventional: ConventionalCommit,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pr_number: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pr_url: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangelogGroups {
    pub features: Vec<CommitInfo>,
    pub fixes: Vec<CommitInfo>,
    pub chores: Vec<CommitInfo>,
    pub docs: Vec<CommitInfo>,
    pub refactors: Vec<CommitInfo>,
    pub perf: Vec<CommitInfo>,
    pub tests: Vec<CommitInfo>,
    pub build_ci: Vec<CommitInfo>,
    pub other: Vec<CommitInfo>,
}

impl ChangelogGroups {
    fn sections(&self) -> [(&'static str, &Vec<CommitInfo>); 9] {
        [
            ("Features", &self.features),
            ("Fixes", &self.fixes),
            ("Chores", &self.chores),
            ("Docs", &self.docs),
            ("Refactors", &self.refactors),
            ("Perf", &self.perf),
            ("Tests", &self.tests),
            ("Build/CI", &self.build_ci),
            ("Other", &self.other),
        ]
    }

    pub fn count(&self) -> usize {
        self.sections().iter().map(|(_, c)| c.len()).sum()
    }
}

/// Builds a commit from its id and full commit message.
pub fn commit_from_message(
    hash: &str,
    message: &str,
    base_url: &str,
    repository_id: &str,
) -> CommitInfo {
    let first_line = message.split('\n').next().unwrap_or_default();
    let first_line = first_line.strip_suffix('\r').unwrap_or(first_line).trim();
    let subject = if first_line.is_empty() {
        hash
    } else {
        first_line
    };
    let (subject, pr_number) = parse_subject_and_pr(subject);
    CommitInfo {
        hash: hash.to_string(),
        short_hash: hash.chars().take(7).collect(),
        conventional: parse_conventional_commit(&subject),
        pr_url: pr_number.map(|n| format!("{base_url}/_git/{repository_id}/pullrequest/{n}")),
        pr_number,
        subject,
    }
}

/// `Merged PR 123: subject` -> (`subject`, Some(123)).
pub fn parse_subject_and_pr(subject: &str) -> (String, Option<u64>) {
    let parsed = (|| {
        let prefix = subject.get(..9)?;
        if !prefix.eq_ignore_ascii_case("merged pr") {
            return None;
        }
        let rest = &subject[9..];
        let after_ws = rest.trim_start();
        if after_ws.len() == rest.len() {
            return None;
        }
        let digits_len = after_ws.bytes().take_while(u8::is_ascii_digit).count();
        if digits_len == 0 {
            return None;
        }
        let after_colon = after_ws[digits_len..].strip_prefix(':')?;
        if after_colon.is_empty() {
            return None;
        }
        Some((&after_ws[..digits_len], after_colon))
    })();

    let Some((digits, text)) = parsed else {
        return (subject.to_string(), None);
    };
    let trimmed = text.trim();
    let new_subject = if trimmed.is_empty() { subject } else { trimmed };
    let pr_number = digits.parse::<u64>().ok().filter(|n| *n > 0);
    (new_subject.to_string(), pr_number)
}

/// `type(scope)!: description` (type letters only, case-insensitive).
pub fn parse_conventional_commit(subject: &str) -> ConventionalCommit {
    let parsed = (|| {
        let type_len = subject.bytes().take_while(u8::is_ascii_alphabetic).count();
        if type_len == 0 {
            return None;
        }
        let mut rest = &subject[type_len..];
        let mut scope = "";
        if let Some(after_paren) = rest.strip_prefix('(') {
            let close = after_paren.find(')')?;
            if close == 0 {
                return None;
            }
            scope = &after_paren[..close];
            rest = &after_paren[close + 1..];
        }
        rest = rest.strip_prefix('!').unwrap_or(rest);
        let description = rest.strip_prefix(':')?;
        if description.is_empty() {
            return None;
        }
        Some((&subject[..type_len], scope, description))
    })();

    match parsed {
        Some((commit_type, scope, description)) => ConventionalCommit {
            commit_type: commit_type.to_lowercase(),
            scope: scope.to_lowercase(),
            description: description.trim().to_string(),
            raw: subject.to_string(),
            is_conventional: true,
        },
        None => ConventionalCommit {
            commit_type: String::new(),
            scope: String::new(),
            description: subject.to_string(),
            raw: subject.to_string(),
            is_conventional: false,
        },
    }
}

pub fn group_commits_by_type(commits: &[CommitInfo]) -> ChangelogGroups {
    let mut groups = ChangelogGroups::default();
    for commit in commits {
        let target = match commit.conventional.commit_type.as_str() {
            "feat" => &mut groups.features,
            "fix" => &mut groups.fixes,
            "chore" => &mut groups.chores,
            "docs" => &mut groups.docs,
            "refactor" => &mut groups.refactors,
            "perf" => &mut groups.perf,
            "test" => &mut groups.tests,
            "build" | "ci" => &mut groups.build_ci,
            _ => &mut groups.other,
        };
        target.push(commit.clone());
    }
    groups
}

fn format_commit_bullet(commit: &CommitInfo) -> String {
    let pr_suffix = match (commit.pr_number, &commit.pr_url) {
        (Some(n), Some(url)) => format!(" ([PR {n}]({url}))"),
        (Some(n), None) => format!(" (PR {n})"),
        _ => String::new(),
    };
    let hash_suffix = if commit.pr_url.is_some() {
        String::new()
    } else {
        format!(" (`{}`)", commit.short_hash)
    };
    format!("- {}{pr_suffix}{hash_suffix}", commit.subject)
}

pub struct MarkdownParams<'a> {
    pub pipeline_name: &'a str,
    pub branch: &'a str,
    pub from_run_id: Option<u64>,
    pub to_run_id: Option<u64>,
    pub from_commit: &'a str,
    pub to_commit: &'a str,
    pub area: &'a str,
    pub included: &'a ChangelogGroups,
    pub manual_review: &'a [CommitInfo],
}

pub fn short(sha: &str) -> String {
    sha.chars().take(7).collect()
}

pub fn range_label(run_id: Option<u64>, commit: &str) -> String {
    match run_id.filter(|id| *id > 0) {
        Some(id) => format!("run #{id}"),
        None => short(commit),
    }
}

pub fn build_markdown_changelog(p: &MarkdownParams) -> String {
    let area_suffix = if p.area.is_empty() {
        String::new()
    } else {
        format!(" | area: {}", p.area)
    };
    let mut lines = vec![
        "## Release Changelog".to_string(),
        String::new(),
        format!("Pipeline: {}", p.pipeline_name),
        format!("Branch: {}", p.branch),
        format!(
            "Range: {} -> {}{area_suffix}",
            range_label(p.from_run_id, p.from_commit),
            range_label(p.to_run_id, p.to_commit)
        ),
        format!("Commits: {}..{}", short(p.from_commit), short(p.to_commit)),
        String::new(),
    ];

    let mut append = |title: &str, commits: &[CommitInfo]| {
        if commits.is_empty() {
            return;
        }
        lines.push(format!("### {title}"));
        lines.extend(commits.iter().map(format_commit_bullet));
        lines.push(String::new());
    };
    for (title, commits) in p.included.sections() {
        append(title, commits);
    }
    append("Needs Manual Review", p.manual_review);

    if lines.last().is_some_and(String::is_empty) {
        lines.pop();
    }
    lines.join("\n")
}

/// Validates and lowercases a `--to` commit SHA.
pub fn normalize_commit_sha(value: &str) -> Result<String> {
    let trimmed = value.trim().to_lowercase();
    let valid = (7..=40).contains(&trimmed.len()) && trimmed.bytes().all(|b| b.is_ascii_hexdigit());
    if !valid {
        bail!("Invalid commit SHA '{value}'.");
    }
    Ok(trimmed)
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: &str = "https://dev.azure.com/org/proj";

    fn commit(hash: &str, message: &str) -> CommitInfo {
        commit_from_message(hash, message, BASE, "repo-id")
    }

    #[test]
    fn parses_conventional_commits() {
        let c = parse_conventional_commit("Feat(UI)!: Add dark mode ");
        assert_eq!(
            (
                c.commit_type.as_str(),
                c.scope.as_str(),
                c.description.as_str()
            ),
            ("feat", "ui", "Add dark mode")
        );
        assert!(c.is_conventional);
        assert!(parse_conventional_commit("fix:x").is_conventional);
        assert!(parse_conventional_commit("feat: ").is_conventional);
        for not in [
            "feat:",
            "feat(): x",
            "feat(ui x",
            "1fix: x",
            "Update readme",
            "feat-x: y",
        ] {
            let c = parse_conventional_commit(not);
            assert!(!c.is_conventional, "{not}");
            assert_eq!(c.description, not);
            assert_eq!(c.commit_type, "");
        }
    }

    #[test]
    fn parses_merged_pr_subjects() {
        assert_eq!(
            parse_subject_and_pr("Merged PR 42: feat(api): add endpoint"),
            ("feat(api): add endpoint".into(), Some(42))
        );
        assert_eq!(
            parse_subject_and_pr("merged pr  7:x"),
            ("x".into(), Some(7))
        );
        assert_eq!(
            parse_subject_and_pr("Merged PR 0: zero"),
            ("zero".into(), None)
        );
        assert_eq!(
            parse_subject_and_pr("Merged PR 5:   "),
            ("Merged PR 5:   ".into(), Some(5))
        );
        assert_eq!(
            parse_subject_and_pr("Merged PR42: x"),
            ("Merged PR42: x".into(), None)
        );

        let c = commit("abcdef1234567", "Merged PR 9: fix: crash\r\n\nbody");
        assert_eq!(c.subject, "fix: crash");
        assert_eq!(c.short_hash, "abcdef1");
        assert_eq!(
            c.pr_url.as_deref(),
            Some("https://dev.azure.com/org/proj/_git/repo-id/pullrequest/9")
        );
        assert_eq!(commit("abc", "  \nbody").subject, "abc");
    }

    #[test]
    fn groups_and_renders_markdown() {
        let commits = vec![
            commit("1111111aaaa", "Merged PR 12: feat(ui): new button"),
            commit("2222222bbbb", "fix: crash"),
            commit("3333333cccc", "ci: pipeline tweak"),
            commit("4444444dddd", "build: bump"),
            commit("5555555eeee", "Random change"),
            commit("6666666ffff", "chore(deps): bump"),
        ];
        let groups = group_commits_by_type(&commits);
        assert_eq!(groups.count(), 6);
        assert_eq!(groups.build_ci.len(), 2);

        let manual = vec![commit("7777777gggg", "docs: readme")];
        let md = build_markdown_changelog(&MarkdownParams {
            pipeline_name: "tapio-app",
            branch: "master",
            from_run_id: Some(100),
            to_run_id: None,
            from_commit: "aaaaaaaaaa",
            to_commit: "bbbbbbbbbb",
            area: "frontend",
            included: &groups,
            manual_review: &manual,
        });
        let expected = "## Release Changelog

Pipeline: tapio-app
Branch: master
Range: run #100 -> bbbbbbb | area: frontend
Commits: aaaaaaa..bbbbbbb

### Features
- feat(ui): new button ([PR 12](https://dev.azure.com/org/proj/_git/repo-id/pullrequest/12))

### Fixes
- fix: crash (`2222222`)

### Chores
- chore(deps): bump (`6666666`)

### Build/CI
- ci: pipeline tweak (`3333333`)
- build: bump (`4444444`)

### Other
- Random change (`5555555`)

### Needs Manual Review
- docs: readme (`7777777`)";
        assert_eq!(md, expected);

        let empty = build_markdown_changelog(&MarkdownParams {
            pipeline_name: "p",
            branch: "main",
            from_run_id: Some(1),
            to_run_id: Some(2),
            from_commit: "aaaaaaa",
            to_commit: "bbbbbbb",
            area: "",
            included: &ChangelogGroups::default(),
            manual_review: &[],
        });
        assert!(empty.ends_with("Range: run #1 -> run #2\nCommits: aaaaaaa..bbbbbbb"));
    }

    #[test]
    fn validates_commit_shas() {
        assert_eq!(normalize_commit_sha(" ABCDEF1 ").unwrap(), "abcdef1");
        assert!(normalize_commit_sha("abc").is_err());
        assert!(normalize_commit_sha("xyz1234").is_err());
    }
}
