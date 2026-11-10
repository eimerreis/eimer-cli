//! `release changelog`: grouped changelog between production releases of a pipeline.

use anyhow::{Context, Result, bail};
use clap::Args;
use serde::Serialize;

use super::api::{PipelineRun, build_run_url, resolve_pipeline_by_name};
use super::areas::{filter_commits_by_area, merge_area_configs};
use super::changelog_format::{
    ChangelogGroups, CommitInfo, MarkdownParams, build_markdown_changelog, group_commits_by_type,
    normalize_commit_sha,
};
use super::client::AzureDevOpsClient;
use super::config::{load_config, mask_webhook_url, resolve_channels, save_config};
use super::prod_stage::{ProdStageChecker, normalize_text};
use super::runs::{Runs, load_remote_commit_range, strip_heads};
use super::teams_review::review_teams_markdown;
use super::teams_webhook::{TeamsPostInput, post_changelog_to_teams};
use super::util::is_interactive_terminal;
use crate::core::error::HintExt;
use crate::core::process::{NoClipboard, copy_to_clipboard};
use crate::core::{prompt, ui};

#[derive(Args)]
pub struct ChangelogArgs {
    /// Pipeline name (alternative to --pipeline)
    #[arg(value_name = "PIPELINE")]
    positional: Vec<String>,
    /// Pipeline name
    #[arg(long, short = 'p')]
    pipeline: Option<String>,
    /// Branch ref override (default: refs/heads/master, or release.defaultBranch from config)
    #[arg(long, short = 'b')]
    branch: Option<String>,
    /// From run ID override (defaults to latest prod-success run on the branch)
    #[arg(long, short = 'f', value_parser = clap::value_parser!(u64).range(1..))]
    from: Option<u64>,
    /// To commit SHA override (default: latest successful pipeline run on the branch)
    #[arg(long, short = 't')]
    to: Option<String>,
    /// Area filter from predefined configs
    #[arg(long, short = 'a')]
    area: Option<String>,
    /// Prod stage identifier/name override; remembered per pipeline once it finds a release
    #[arg(long, visible_alias = "prod-stage")]
    prod_stage_name: Option<String>,
    /// Print only, do not copy changelog to clipboard
    #[arg(long)]
    no_copy: bool,
    /// Post changelog to Microsoft Teams incoming webhook URL
    #[arg(long, value_name = "URL")]
    post_webhook: Option<String>,
    /// Named Teams channel from release config
    #[arg(long)]
    channel: Option<String>,
    /// Print machine-readable JSON
    #[arg(long, short = 'j')]
    json: bool,
}

fn trimmed(value: &Option<String>) -> Option<String> {
    value
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(String::from)
}

pub fn run(args: ChangelogArgs) -> Result<()> {
    changelog(args).context("Failed to build changelog").hint(
        "Verify SYSTEM_ACCESSTOKEN/SYSTEM_COLLECTIONURI/SYSTEM_TEAMPROJECT in CI or local Azure defaults with `release configure --show`.",
    )
}

#[derive(Serialize)]
struct JsonPipeline<'a> {
    id: u64,
    name: &'a str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct JsonRange<'a> {
    from_run_id: u64,
    to_run_id: Option<u64>,
    from_commit: &'a str,
    to_commit: &'a str,
    branch: &'a str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct JsonOutput<'a> {
    pipeline: JsonPipeline<'a>,
    range: JsonRange<'a>,
    area: Option<&'a str>,
    total_commits: usize,
    included_count: usize,
    manual_review_count: usize,
    posted_to_webhook: bool,
    posted_format: Option<&'a str>,
    posted_channel: Option<&'a str>,
    grouped: &'a ChangelogGroups,
    manual_review: &'a [CommitInfo],
    markdown: &'a str,
}

fn changelog(args: ChangelogArgs) -> Result<()> {
    let interactive = is_interactive_terminal();
    let ci_mode = is_ci_environment() || !interactive;
    let json = args.json;

    let config = load_config()?;
    let client = ui::spin_unless(
        json,
        "Loading Azure DevOps context",
        AzureDevOpsClient::create,
    )?;
    let context = client.context.clone();

    let default_pipeline = config.default_pipeline();
    let branch = trimmed(&args.branch)
        .or_else(|| Some(config.default_branch()).filter(|b| !b.is_empty()))
        .unwrap_or_else(|| "refs/heads/master".into());
    let branch_display = strip_heads(&branch);
    let area_configs = merge_area_configs(config.areas.as_ref());
    let channels = resolve_channels(&config);

    let mut pipeline_name =
        trimmed(&args.pipeline).unwrap_or_else(|| args.positional.join(" ").trim().to_string());
    if pipeline_name.is_empty() {
        pipeline_name = default_pipeline;
    }
    if pipeline_name.is_empty() && interactive {
        pipeline_name = prompt::text("Pipeline name", None, |v| {
            v.trim()
                .is_empty()
                .then(|| "Pipeline name is required".to_string())
        })?;
    }
    if pipeline_name.is_empty() {
        bail!(
            "Missing pipeline name. Pass `--pipeline <name>` or configure `release.defaultPipeline` via `release configure`."
        );
    }

    let pipeline = ui::spin_unless(json, "Resolving pipeline", || {
        resolve_pipeline_by_name(&client, &pipeline_name)
    })?;
    let display_pipeline = pipeline
        .name
        .clone()
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| pipeline_name.clone());

    // --prod-stage-name > remembered for this pipeline > global release.prodStageName > heuristics
    let flag_stage = trimmed(&args.prod_stage_name);
    let prod_stage_name = flag_stage
        .clone()
        .or_else(|| config.prod_stage_for(&display_pipeline))
        .or_else(|| config.prod_stage_for(&pipeline_name))
        .unwrap_or_else(|| config.prod_stage_name());
    let mut checker = ProdStageChecker::new(&client, &prod_stage_name);
    let runs = Runs {
        client: &client,
        pipeline_id: pipeline.id,
        branch: &branch,
    };

    let mut from_run = ui::spin_unless(json, "Resolving source release run", || {
        runs.resolve_from_run(args.from, &mut checker, &branch_display)
    })?;
    // The explicit stage just identified a prod release run (not bypassed by --from),
    // so it is valid for this pipeline: remember it. Never write config in CI.
    if let (Some(stage), None, false) = (&flag_stage, args.from, ci_mode) {
        remember_prod_stage(&display_pipeline, stage, json)?;
    }
    let mut from_commit = from_run.source_version();
    if from_commit.is_empty() {
        bail!("Selected from run #{} has no sourceVersion.", from_run.id);
    }
    let repository_id = from_run
        .repository
        .as_ref()
        .and_then(|r| r.id.as_deref())
        .unwrap_or_default()
        .trim()
        .to_string();
    if repository_id.is_empty() {
        bail!("Selected from run #{} has no repository ID.", from_run.id);
    }

    let to_flag = trimmed(&args.to);
    let target_run = match &to_flag {
        Some(_) => None,
        None => ui::spin_unless(
            json,
            &format!("Resolving target {branch_display} run"),
            || runs.latest_succeeded_run(),
        )?,
    };
    let mut to_commit = match &to_flag {
        Some(sha) => normalize_commit_sha(sha)?,
        None => target_run
            .as_ref()
            .map(PipelineRun::source_version)
            .unwrap_or_default(),
    };
    let mut to_run_id = if to_flag.is_some() {
        None
    } else {
        target_run.as_ref().map(|r| r.id)
    };
    if to_commit.is_empty() {
        bail!(
            "Could not resolve target commit from latest successful {branch_display} pipeline run."
        );
    }

    // Nothing new since the last prod release: show the previous release's range instead.
    if from_commit.eq_ignore_ascii_case(&to_commit) {
        let current = target_run.clone().unwrap_or_else(|| from_run.clone());
        let previous = runs.previous_prod_release_run(current.id, &mut checker)?;
        let Some(previous) = previous else {
            bail!(
                "No new {branch_display} commits and no previous successful prod release run found for fallback."
            );
        };
        let previous_commit = previous.source_version();
        if previous_commit.is_empty() {
            bail!("Fallback run #{} has no sourceVersion.", previous.id);
        }
        let current_commit = current.source_version();
        if current_commit.is_empty() {
            bail!("Current release run #{} has no sourceVersion.", current.id);
        }
        to_run_id = Some(current.id);
        from_run = previous;
        from_commit = previous_commit;
        to_commit = current_commit;
    }

    let commits = ui::spin_unless(json, "Loading commit range", || {
        load_remote_commit_range(
            &client,
            &context.base_url,
            &repository_id,
            &from_commit,
            &to_commit,
        )
    })?;
    let total_commits = commits.len();

    let filtered = filter_commits_by_area(commits, args.area.as_deref(), &area_configs)?;
    let grouped = group_commits_by_type(&filtered.included);
    let area_name = filtered.area_name.as_str();

    let mut markdown = build_markdown_changelog(&MarkdownParams {
        pipeline_name: &display_pipeline,
        branch: &branch_display,
        from_run_id: Some(from_run.id),
        to_run_id,
        from_commit: &from_commit,
        to_commit: &to_commit,
        area: area_name,
        included: &grouped,
        manual_review: &filtered.manual_review,
    });

    let post_webhook = trimmed(&args.post_webhook);
    let channel = trimmed(&args.channel);
    if ci_mode && post_webhook.is_none() && channel.is_none() {
        bail!(
            "Missing webhook target in CI mode. Pass `--post-webhook <url>` or `--channel <name>`."
        );
    }

    let target = resolve_webhook_target(
        post_webhook.as_deref(),
        channel.as_deref(),
        &channels,
        !json && interactive,
    )?;
    if ci_mode && target.webhook_url.is_empty() {
        bail!(
            "Missing Teams webhook in CI mode. Pass `--post-webhook <url>` or `--channel <name>` with configured channels."
        );
    }

    let mut skipped_post = false;
    if !target.webhook_url.is_empty() && !json && interactive {
        let review = review_teams_markdown(&markdown)?;
        markdown = review.markdown;
        skipped_post = !review.should_post;
    }

    let mut posted_format = None;
    if !target.webhook_url.is_empty() && !skipped_post {
        let input = TeamsPostInput {
            webhook_url: &target.webhook_url,
            pipeline_name: &display_pipeline,
            from_run_id: Some(from_run.id),
            to_run_id,
            from_commit: &from_commit,
            to_commit: &to_commit,
            area: area_name,
            pipeline_run_url: to_run_id.map(|id| build_run_url(&context, id)),
            markdown: &markdown,
        };
        posted_format = Some(ui::spin_unless(json, "Posting changelog to Teams", || {
            post_changelog_to_teams(&input)
        })?);
    }

    if json {
        return ui::print_json(&JsonOutput {
            pipeline: JsonPipeline {
                id: pipeline.id,
                name: &display_pipeline,
            },
            range: JsonRange {
                from_run_id: from_run.id,
                to_run_id,
                from_commit: &from_commit,
                to_commit: &to_commit,
                branch: &branch_display,
            },
            area: Some(area_name).filter(|a| !a.is_empty()),
            total_commits,
            included_count: grouped.count(),
            manual_review_count: filtered.manual_review.len(),
            posted_to_webhook: posted_format.is_some(),
            posted_format,
            posted_channel: Some(target.channel_name.as_str()).filter(|c| !c.is_empty()),
            grouped: &grouped,
            manual_review: &filtered.manual_review,
            markdown: &markdown,
        });
    }

    println!("{markdown}");

    if !args.no_copy && interactive {
        match ui::with_spinner("Copying changelog to clipboard", || {
            copy_to_clipboard(&markdown)
        }) {
            Ok(()) => ui::print_success("Copied changelog to clipboard."),
            Err(err) if err.downcast_ref::<NoClipboard>().is_some() => {
                ui::print_warning(&err.to_string(), Some("Pass --no-copy to skip copying."));
            }
            Err(err) => return Err(err),
        }
    } else if !args.no_copy {
        ui::print_info("Skipped clipboard copy in non-interactive mode.", None);
    }

    if let Some(format) = posted_format {
        let label = if target.channel_name.is_empty() {
            " webhook".to_string()
        } else {
            format!(" channel '{}'", target.channel_name)
        };
        ui::print_success(&format!("Posted changelog to Teams{label} ({format})."));
    } else if skipped_post {
        ui::print_info("Skipped Teams webhook post.", None);
    }
    Ok(())
}

fn remember_prod_stage(pipeline: &str, stage: &str, json: bool) -> Result<()> {
    let mut config = load_config()?;
    if config.remember_prod_stage(pipeline, stage) {
        save_config(&config)?;
        if !json {
            ui::print_info(
                &format!("Remembered prod stage '{stage}' for '{pipeline}'."),
                Some("Next time --prod-stage-name is not needed for this pipeline."),
            );
        }
    }
    Ok(())
}

fn is_ci_environment() -> bool {
    [
        "CI",
        "TF_BUILD",
        "SYSTEM_ACCESSTOKEN",
        "SYSTEM_COLLECTIONURI",
        "SYSTEM_TEAMPROJECT",
    ]
    .iter()
    .any(|name| std::env::var(name).is_ok_and(|v| !v.trim().is_empty()))
}

struct WebhookTarget {
    webhook_url: String,
    channel_name: String,
}

/// `--post-webhook` -> `--channel` -> single configured channel -> interactive picker.
fn resolve_webhook_target(
    ad_hoc_url: Option<&str>,
    selected_channel: Option<&str>,
    channels: &[(String, String)],
    use_interactive_prompt: bool,
) -> Result<WebhookTarget> {
    let target = |(name, url): &(String, String)| WebhookTarget {
        channel_name: name.clone(),
        webhook_url: url.clone(),
    };
    if let Some(url) = ad_hoc_url {
        return Ok(WebhookTarget {
            webhook_url: url.into(),
            channel_name: "ad-hoc".into(),
        });
    }
    if channels.is_empty() {
        return Ok(WebhookTarget {
            webhook_url: String::new(),
            channel_name: String::new(),
        });
    }
    let available = || {
        let mut names: Vec<&str> = channels.iter().map(|(n, _)| n.as_str()).collect();
        names.sort();
        names.join(", ")
    };

    if let Some(selected) = selected_channel {
        return match channels
            .iter()
            .find(|(name, _)| normalize_text(name) == normalize_text(selected))
        {
            Some(entry) => Ok(target(entry)),
            None => bail!(
                "Unknown Teams channel '{selected}'. Available channels: {}",
                available()
            ),
        };
    }
    if channels.len() == 1 {
        return Ok(target(&channels[0]));
    }
    if !use_interactive_prompt {
        bail!(
            "Multiple Teams channels configured. Pass --channel <name>. Available channels: {}",
            available()
        );
    }

    let mut sorted: Vec<&(String, String)> = channels.iter().collect();
    sorted.sort_by(|a, b| a.0.cmp(&b.0));
    let labels: Vec<String> = sorted
        .iter()
        .map(|(name, url)| format!("{name} ({})", mask_webhook_url(url)))
        .collect();
    let default = sorted
        .iter()
        .position(|(name, _)| *name == channels[0].0)
        .unwrap_or(0);
    let index = prompt::select("Choose Teams channel", &labels, default)?;
    Ok(target(sorted[index]))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn channels() -> Vec<(String, String)> {
        vec![
            ("default".into(), "https://default-hook".into()),
            ("Frontend".into(), "https://fe-hook".into()),
        ]
    }

    #[test]
    fn webhook_target_resolution() {
        let t =
            resolve_webhook_target(Some("https://adhoc"), Some("x"), &channels(), false).unwrap();
        assert_eq!(
            (t.channel_name.as_str(), t.webhook_url.as_str()),
            ("ad-hoc", "https://adhoc")
        );

        let t = resolve_webhook_target(None, Some(" frontend "), &channels(), false).unwrap();
        assert_eq!(t.channel_name, "Frontend");

        let err = resolve_webhook_target(None, Some("nope"), &channels(), false)
            .err()
            .unwrap();
        assert_eq!(
            err.to_string(),
            "Unknown Teams channel 'nope'. Available channels: Frontend, default"
        );

        let err = resolve_webhook_target(None, None, &channels(), false)
            .err()
            .unwrap();
        assert!(
            err.to_string()
                .starts_with("Multiple Teams channels configured.")
        );

        let single = vec![("default".to_string(), "https://d".to_string())];
        let t = resolve_webhook_target(None, None, &single, false).unwrap();
        assert_eq!(t.webhook_url, "https://d");

        let t = resolve_webhook_target(None, Some("x"), &[], false).unwrap();
        assert!(t.webhook_url.is_empty() && t.channel_name.is_empty());
    }
}
