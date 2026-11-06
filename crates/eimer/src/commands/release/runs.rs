//! Release run resolution and commit ranges for `release changelog`.

use std::collections::HashSet;

use anyhow::{Result, bail};

use super::api::{PipelineRun, RunQuery, load_commits, load_pipeline_runs, load_run_by_id};
use super::changelog_format::{CommitInfo, commit_from_message, short};
use super::client::AzureDevOpsClient;
use super::prod_stage::{ProdStageChecker, normalize_text};

pub fn strip_heads(branch: &str) -> String {
    match branch.get(..11) {
        Some(prefix) if prefix.eq_ignore_ascii_case("refs/heads/") => branch[11..].to_string(),
        _ => branch.to_string(),
    }
}

fn normalize_branch(value: &str) -> String {
    normalize_text(&strip_heads(value))
}

pub struct Runs<'a> {
    pub client: &'a AzureDevOpsClient,
    pub pipeline_id: u64,
    pub branch: &'a str,
}

impl Runs<'_> {
    fn completed(&self, top: u32) -> Result<Vec<PipelineRun>> {
        load_pipeline_runs(
            self.client,
            RunQuery {
                pipeline_id: self.pipeline_id,
                top: Some(top),
                status: Some("completed"),
                result: None,
                branch: Some(self.branch),
            },
        )
    }

    pub fn latest_succeeded_run(&self) -> Result<Option<PipelineRun>> {
        let runs = load_pipeline_runs(
            self.client,
            RunQuery {
                pipeline_id: self.pipeline_id,
                top: Some(1),
                status: Some("completed"),
                result: Some("succeeded"),
                branch: Some(self.branch),
            },
        )?;
        Ok(runs.into_iter().next())
    }

    pub fn resolve_from_run(
        &self,
        from_run_id: Option<u64>,
        checker: &mut ProdStageChecker,
        branch_display: &str,
    ) -> Result<PipelineRun> {
        if let Some(id) = from_run_id {
            let run = load_run_by_id(self.client, id)?;
            validate_run_branch_success(&run, self.pipeline_id, branch_display)?;
            return Ok(run);
        }
        match self.latest_prod_release_run(checker)? {
            Some(run) => Ok(run),
            None => bail!(
                "No successful prod release run found on {branch_display}. Configure release.prodStageName via `release configure` or pass --from <runId>."
            ),
        }
    }

    fn latest_prod_release_run(
        &self,
        checker: &mut ProdStageChecker,
    ) -> Result<Option<PipelineRun>> {
        let latest = self.completed(1)?;
        if latest.len() == 1 && checker.has_successful_prod_stage(latest[0].id)? {
            return Ok(latest.into_iter().next());
        }
        for run in self.completed(200)? {
            if checker.has_successful_prod_stage(run.id)? {
                return Ok(Some(run));
            }
        }
        Ok(None)
    }

    pub fn previous_prod_release_run(
        &self,
        current_run_id: u64,
        checker: &mut ProdStageChecker,
    ) -> Result<Option<PipelineRun>> {
        let mut passed_current = false;
        for run in self.completed(200)? {
            if !checker.has_successful_prod_stage(run.id)? {
                continue;
            }
            if !passed_current {
                if run.id == current_run_id {
                    passed_current = true;
                }
                continue;
            }
            return Ok(Some(run));
        }
        Ok(None)
    }
}

fn validate_run_branch_success(
    run: &PipelineRun,
    pipeline_id: u64,
    expected_branch: &str,
) -> Result<()> {
    if let Some(def_id) = run.definition.as_ref().and_then(|d| d.id)
        && def_id != 0
        && def_id != pipeline_id
    {
        bail!(
            "Run #{} does not belong to selected pipeline ID {pipeline_id}.",
            run.id
        );
    }
    if normalize_text(run.status.as_deref().unwrap_or_default()) != "completed" {
        bail!("Run #{} is not completed.", run.id);
    }
    if normalize_text(run.result.as_deref().unwrap_or_default()) != "succeeded" {
        bail!("Run #{} is not succeeded.", run.id);
    }
    let source_branch = run.source_branch.as_deref().unwrap_or_default();
    if normalize_branch(source_branch) != expected_branch.to_lowercase() {
        bail!(
            "Run #{} is on branch '{source_branch}', expected {expected_branch}.",
            run.id
        );
    }
    Ok(())
}

/// Walks commit history back from `to_commit` until `from_commit` (exclusive), trying
/// first-parent history first, then full history.
pub fn load_remote_commit_range(
    client: &AzureDevOpsClient,
    base_url: &str,
    repository_id: &str,
    from_commit: &str,
    to_commit: &str,
) -> Result<Vec<CommitInfo>> {
    const PAGE_SIZE: usize = 100;
    const MAX_PAGES: usize = 20;

    let load_with_mode = |history_mode: &str| -> Result<Option<Vec<CommitInfo>>> {
        let mut commits = Vec::new();
        let mut seen = HashSet::new();
        let mut skip = 0;
        for _ in 0..MAX_PAGES {
            let items = load_commits(
                client,
                repository_id,
                vec![
                    ("searchCriteria.itemVersion.version", Some(to_commit.into())),
                    (
                        "searchCriteria.itemVersion.versionType",
                        Some("commit".into()),
                    ),
                    ("searchCriteria.$top", Some(PAGE_SIZE.to_string())),
                    ("searchCriteria.$skip", Some(skip.to_string())),
                    ("searchCriteria.historyMode", Some(history_mode.into())),
                ],
            )?;
            if items.is_empty() {
                return Ok(None);
            }
            skip += items.len();
            for item in items {
                let hash = item
                    .commit_id
                    .as_deref()
                    .unwrap_or_default()
                    .trim()
                    .to_string();
                if hash.is_empty() || !seen.insert(hash.clone()) {
                    continue;
                }
                if hash.eq_ignore_ascii_case(from_commit) {
                    return Ok(Some(commits));
                }
                let message = item.comment.as_deref().unwrap_or_default();
                commits.push(commit_from_message(&hash, message, base_url, repository_id));
            }
        }
        Ok(None)
    };

    if let Some(range) = load_with_mode("firstParent")? {
        return Ok(range);
    }
    if let Some(range) = load_with_mode("fullHistory")? {
        return Ok(range);
    }
    bail!(
        "Could not resolve commit range from {} to {}. History may have diverged or been rewritten on the release branch. Pass --from <runId> from current branch history or set --to <sha>.",
        short(from_commit),
        short(to_commit)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_from_run() {
        let mut run = PipelineRun {
            id: 7,
            status: Some("completed".into()),
            result: Some("succeeded".into()),
            source_branch: Some("refs/heads/Master".into()),
            ..Default::default()
        };
        assert!(validate_run_branch_success(&run, 1, "master").is_ok());
        run.result = Some("failed".into());
        assert_eq!(
            validate_run_branch_success(&run, 1, "master")
                .unwrap_err()
                .to_string(),
            "Run #7 is not succeeded."
        );
        run.result = Some("succeeded".into());
        assert_eq!(
            validate_run_branch_success(&run, 1, "main")
                .unwrap_err()
                .to_string(),
            "Run #7 is on branch 'refs/heads/Master', expected main."
        );
        assert_eq!(strip_heads("REFS/HEADS/release/2.0"), "release/2.0");
        assert_eq!(strip_heads("main"), "main");
    }
}
