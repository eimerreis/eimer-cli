//! Azure DevOps scoping: which org/project `az` commands target, git remote parsing, URLs.

use super::context::ResolvedContext;
use super::process::try_run_text;

/// Org/project parsed from a git remote or a context.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AzureProject {
    pub org: String,
    pub project: String,
    /// e.g. `https://dev.azure.com/org/project`
    pub base_url: String,
}

/// Scope for `az` invocations. With a context, org/project are passed explicitly;
/// without one, `az` detects them from the git remote (`--detect true`).
#[derive(Debug, Clone, Default)]
pub struct AzScope {
    pub context: Option<ResolvedContext>,
}

impl AzScope {
    /// Scope without a context: always `--detect true`.
    pub fn detect() -> Self {
        Self { context: None }
    }

    pub fn with_context(context: Option<ResolvedContext>) -> Self {
        Self { context }
    }

    /// Args for `az` commands that accept `--org` and `--project`.
    pub fn args(&self) -> Vec<String> {
        self.build_args(true)
    }

    /// Args for `az boards work-item show/update/relation`, which do not accept `--project`.
    pub fn org_args(&self) -> Vec<String> {
        self.build_args(false)
    }

    fn build_args(&self, with_project: bool) -> Vec<String> {
        match &self.context {
            None => vec!["--detect".into(), "true".into()],
            Some(ctx) => {
                let mut args = vec!["--org".into(), ctx.org_url.clone()];
                if with_project {
                    args.push("--project".into());
                    args.push(ctx.project.clone());
                }
                args
            }
        }
    }

    /// The project this scope targets: the context if set, else the cwd's git remote.
    pub fn project(&self) -> Option<AzureProject> {
        match &self.context {
            Some(ctx) => Some(AzureProject {
                org: ctx.org.clone(),
                project: ctx.project.clone(),
                base_url: format!("{}/{}", ctx.org_url, encode_path_segment(&ctx.project)),
            }),
            None => remote_project(),
        }
    }

    pub fn work_item_url(&self, id: u64) -> String {
        self.project()
            .map(|p| work_item_url(&p, id))
            .unwrap_or_default()
    }
}

pub fn work_item_url(project: &AzureProject, id: u64) -> String {
    format!("{}/_workitems/edit/{id}", project.base_url)
}

/// Org/project of the cwd's `origin` remote, if it is an Azure DevOps remote.
pub fn remote_project() -> Option<AzureProject> {
    let url = try_run_text(["git", "remote", "get-url", "origin"])?;
    parse_remote(url.trim())
}

/// Parses HTTPS, SSH and legacy visualstudio.com SSH remotes.
pub fn parse_remote(remote: &str) -> Option<AzureProject> {
    let make = |org: &str, project: &str, base: String| AzureProject {
        org: org.into(),
        project: project.into(),
        base_url: base,
    };

    // https://[user@]dev.azure.com/{org}/{project}/_git/{repo}
    if let Some(rest) = remote.strip_prefix("https://") {
        let rest = rest
            .split_once('@')
            .filter(|(user, _)| !user.contains('/'))
            .map_or(rest, |(_, r)| r);
        if let Some(path) = rest.strip_prefix("dev.azure.com/") {
            let parts: Vec<&str> = path.split('/').collect();
            if parts.len() >= 4 && parts[2].eq_ignore_ascii_case("_git") {
                return Some(make(
                    parts[0],
                    parts[1],
                    format!("https://dev.azure.com/{}/{}", parts[0], parts[1]),
                ));
            }
        }
        // https://{org}.visualstudio.com/{project}/_git/{repo}
        if let Some((host, path)) = rest.split_once('/') {
            if let Some(org) = host.strip_suffix(".visualstudio.com") {
                let parts: Vec<&str> = path.split('/').collect();
                if parts.len() >= 3 && parts[1].eq_ignore_ascii_case("_git") {
                    return Some(make(
                        org,
                        parts[0],
                        format!("https://{org}.visualstudio.com/{}", parts[0]),
                    ));
                }
            }
        }
        return None;
    }

    let ssh = remote.strip_prefix("ssh://").unwrap_or(remote);
    // git@ssh.dev.azure.com:v3/{org}/{project}/{repo}
    if let Some(path) = ssh.strip_prefix("git@ssh.dev.azure.com:v3/") {
        let parts: Vec<&str> = path.split('/').collect();
        if parts.len() >= 3 {
            return Some(make(
                parts[0],
                parts[1],
                format!("https://dev.azure.com/{}/{}", parts[0], parts[1]),
            ));
        }
    }
    // {user}@{org}.vs-ssh.visualstudio.com:v3/{org}/{project}/{repo}
    if let Some((_, host_path)) = ssh.split_once('@') {
        if let Some((host, path)) = host_path.split_once(":v3/") {
            if let Some(org) = host.strip_suffix(".vs-ssh.visualstudio.com") {
                let parts: Vec<&str> = path.split('/').collect();
                if parts.len() >= 3 {
                    return Some(make(
                        org,
                        parts[1],
                        format!("https://{org}.visualstudio.com/{}", parts[1]),
                    ));
                }
            }
        }
    }
    None
}

/// Repository name from an Azure DevOps remote (last path segment, without `.git`).
pub fn parse_repo_name(remote: &str) -> Option<String> {
    let last = remote.trim().trim_end_matches('/').rsplit('/').next()?;
    let name = last.strip_suffix(".git").unwrap_or(last);
    (!name.is_empty()).then(|| name.to_string())
}

/// Case-insensitive org/project comparison (tolerates URL-encoded project names).
pub fn same_project(a: &AzureProject, b: &AzureProject) -> bool {
    a.org.eq_ignore_ascii_case(&b.org)
        && decode_path_segment(&a.project).eq_ignore_ascii_case(&decode_path_segment(&b.project))
}

pub fn encode_path_segment(value: &str) -> String {
    value.replace('%', "%25").replace(' ', "%20")
}

pub fn decode_path_segment(value: &str) -> String {
    value.replace("%20", " ").replace("%25", "%")
}

/// Escapes a value for use inside a single-quoted WIQL string literal.
pub fn escape_wiql(value: &str) -> String {
    value.replace('\'', "''")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_remotes() {
        let https = parse_remote("https://tapioone@dev.azure.com/tapioone/tapio/_git/app").unwrap();
        assert_eq!(
            (https.org.as_str(), https.project.as_str()),
            ("tapioone", "tapio")
        );
        assert_eq!(https.base_url, "https://dev.azure.com/tapioone/tapio");

        let ssh = parse_remote("git@ssh.dev.azure.com:v3/homag-group/onehomag/app").unwrap();
        assert_eq!(
            (ssh.org.as_str(), ssh.project.as_str()),
            ("homag-group", "onehomag")
        );

        let vs = parse_remote("org@org.vs-ssh.visualstudio.com:v3/org/Proj/repo").unwrap();
        assert_eq!(vs.base_url, "https://org.visualstudio.com/Proj");

        assert_eq!(parse_remote("https://github.com/eimerreis/eimer-cli"), None);
        assert_eq!(
            parse_repo_name("git@ssh.dev.azure.com:v3/o/p/my-repo"),
            Some("my-repo".into())
        );
    }

    #[test]
    fn scope_args() {
        assert_eq!(AzScope::detect().args(), vec!["--detect", "true"]);
        let ctx = ResolvedContext {
            name: "t".into(),
            org: "tapioone".into(),
            project: "tapio".into(),
            team: None,
            area_path: None,
            source: super::super::context::ContextSource::Flag,
            rc_path: None,
            org_url: "https://dev.azure.com/tapioone".into(),
        };
        let scope = AzScope::with_context(Some(ctx));
        assert_eq!(
            scope.args(),
            vec![
                "--org",
                "https://dev.azure.com/tapioone",
                "--project",
                "tapio"
            ]
        );
        assert_eq!(
            scope.org_args(),
            vec!["--org", "https://dev.azure.com/tapioone"]
        );
        assert_eq!(
            scope.work_item_url(5),
            "https://dev.azure.com/tapioone/tapio/_workitems/edit/5"
        );
    }

    #[test]
    fn wiql_escaping() {
        assert_eq!(escape_wiql("it's"), "it''s");
    }
}
