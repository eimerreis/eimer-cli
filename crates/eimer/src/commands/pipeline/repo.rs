//! Repository detection (Azure Repos and GitHub remotes) and Azure DevOps run URLs.

use std::sync::OnceLock;

use anyhow::{Result, anyhow};
use serde::Serialize;

use crate::core::azure::remote_project;
use crate::core::process::{run_text, try_run_text};

/// Repository of the cwd, as used for filtering runs and listing definitions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoInfo {
    pub name: String,
    pub repository_filter: String,
    pub repository_type: &'static str,
}

pub fn get_repo_info() -> Result<RepoInfo> {
    let remote = run_text(["git", "remote", "get-url", "origin"])?;
    let remote = remote.trim();
    parse_repo_info(remote)
        .ok_or_else(|| anyhow!("Could not detect repository info from remote URL: {remote}"))
}

/// Last path segment without `.git`; `None` if empty or more segments follow.
fn repo_segment(rest: &str) -> Option<String> {
    let name = rest.strip_suffix(".git").unwrap_or(rest);
    (!name.is_empty() && !name.contains('/')).then(|| name.to_string())
}

fn azure(name: String) -> RepoInfo {
    RepoInfo {
        repository_filter: name.clone(),
        name,
        repository_type: "tfsgit",
    }
}

/// Parses Azure DevOps (HTTPS, SSH, vs-ssh) and GitHub (HTTPS, SSH) remotes.
pub fn parse_repo_info(remote: &str) -> Option<RepoInfo> {
    // ASCII lowercasing keeps byte offsets, so slices of `lower` index into `remote`.
    let lower = remote.to_ascii_lowercase();

    if let Some(rest) = lower.strip_prefix("https://") {
        let offset = remote.len() - rest.len();
        // Skip an optional `user@` (Azure HTTPS remotes usually carry the org as user).
        let host_start = match rest.split_once('/') {
            Some((host, _)) => host.rfind('@').map_or(0, |i| i + 1),
            None => 0,
        };
        let rest_lower = &rest[host_start..];
        let rest_orig = &remote[offset + host_start..];
        let is_azure = rest_lower.starts_with("dev.azure.com/")
            || rest_lower
                .split_once('/')
                .is_some_and(|(host, _)| host.ends_with(".visualstudio.com"));
        if is_azure {
            let idx = rest_lower.rfind("/_git/")?;
            return repo_segment(&rest_orig[idx + "/_git/".len()..]).map(azure);
        }
        if rest_lower.starts_with("github.com/") {
            return github(&rest_orig["github.com/".len()..]);
        }
        return None;
    }

    if lower.starts_with("git@github.com:") {
        return github(&remote["git@github.com:".len()..]);
    }

    let ssh_offset = if lower.starts_with("ssh://") { 6 } else { 0 };
    let ssh_lower = &lower[ssh_offset..];
    let ssh_orig = &remote[ssh_offset..];
    let (user, host_path) = ssh_lower.split_once('@')?;
    if user.is_empty() {
        return None;
    }
    let (host, _) = host_path.split_once(":v3/")?;
    let azure_host = (user == "git" && host == "ssh.dev.azure.com")
        || host
            .strip_suffix(".vs-ssh.visualstudio.com")
            .is_some_and(|org| !org.is_empty() && !org.contains('.'));
    if !azure_host {
        return None;
    }
    let path = &ssh_orig[user.len() + 1 + host.len() + ":v3/".len()..];
    let parts: Vec<&str> = path.splitn(3, '/').collect();
    match parts.as_slice() {
        [org, project, repo] if !org.is_empty() && !project.is_empty() => {
            repo_segment(repo).map(azure)
        }
        _ => None,
    }
}

fn github(path: &str) -> Option<RepoInfo> {
    let (owner, repo) = path.split_once('/')?;
    let repo = repo_segment(repo)?;
    (!owner.is_empty()).then(|| RepoInfo {
        repository_filter: format!("{owner}/{repo}"),
        name: repo,
        repository_type: "github",
    })
}

/// `https://dev.azure.com/{org}/{project}`: from the git remote (what `--detect` uses),
/// else from `az devops configure` defaults, else a placeholder.
fn project_base_url() -> &'static str {
    static BASE: OnceLock<String> = OnceLock::new();
    BASE.get_or_init(|| {
        remote_project()
            .map(|p| p.base_url)
            .or_else(|| {
                try_run_text(["az", "devops", "configure", "--list"])
                    .and_then(|out| parse_az_defaults(&out))
            })
            .unwrap_or_else(|| "https://dev.azure.com/<org>/<project>".into())
    })
}

fn parse_az_defaults(output: &str) -> Option<String> {
    let value = |key: &str| {
        output.lines().find_map(|line| {
            let (k, v) = line.split_once('=')?;
            k.trim()
                .eq_ignore_ascii_case(key)
                .then(|| v.trim().to_string())
        })
    };
    let org = value("organization")?;
    let org = org.trim_end_matches('/');
    let project = value("project")?;
    (!org.is_empty() && !project.is_empty()).then(|| format!("{org}/{project}"))
}

pub fn build_run_url(id: u64) -> String {
    format!("{}/_build/results?buildId={id}", project_base_url())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repo_info() {
        let az = |url: &str| {
            parse_repo_info(url).map(|r| (r.name, r.repository_filter, r.repository_type))
        };
        assert_eq!(
            az("https://dev.azure.com/org/proj/_git/my-repo"),
            Some(("my-repo".into(), "my-repo".into(), "tfsgit"))
        );
        assert_eq!(
            az("https://org@dev.azure.com/org/proj/_git/my-repo.git"),
            Some(("my-repo".into(), "my-repo".into(), "tfsgit"))
        );
        assert_eq!(
            az("https://org.visualstudio.com/proj/_git/Repo"),
            Some(("Repo".into(), "Repo".into(), "tfsgit"))
        );
        assert_eq!(
            az("git@ssh.dev.azure.com:v3/org/proj/repo"),
            Some(("repo".into(), "repo".into(), "tfsgit"))
        );
        assert_eq!(
            az("ssh://git@ssh.dev.azure.com:v3/org/proj/repo.git"),
            Some(("repo".into(), "repo".into(), "tfsgit"))
        );
        assert_eq!(
            az("org@org.vs-ssh.visualstudio.com:v3/org/proj/repo"),
            Some(("repo".into(), "repo".into(), "tfsgit"))
        );
        assert_eq!(
            az("https://github.com/eimerreis/eimer-cli.git"),
            Some(("eimer-cli".into(), "eimerreis/eimer-cli".into(), "github"))
        );
        assert_eq!(
            az("git@github.com:eimerreis/eimer-cli"),
            Some(("eimer-cli".into(), "eimerreis/eimer-cli".into(), "github"))
        );
        assert_eq!(az("https://gitlab.com/a/b"), None);
        assert_eq!(az("git@ssh.dev.azure.com:v3/org/proj"), None);
        assert_eq!(az("https://dev.azure.com/org/proj/_git/a/b"), None);
    }

    #[test]
    fn az_defaults() {
        let out = "\nDefaults in the 'defaults' section:\n\norganization = https://dev.azure.com/org/\nproject = Proj\n";
        assert_eq!(
            parse_az_defaults(out),
            Some("https://dev.azure.com/org/Proj".into())
        );
        assert_eq!(parse_az_defaults("organization = x\n"), None);
    }
}
