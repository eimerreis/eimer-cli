//! `time projects|init|suggest|book|none|status`.

use anyhow::{Result, anyhow, bail};
use clap::Args;

use super::api::{TimeClient, TimeRecordInput};
use super::block::{self, TimeEntry};
use super::repo::{self, ProjectConfig, PullRequest, TimeConfig};
use super::{duration, suggest as suggestion};
use crate::core::error::{HintExt, SilentExit};
use crate::core::ui;

#[derive(Args)]
pub struct ProjectsArgs {
    /// Include completed projects
    #[arg(long, short = 'a')]
    all: bool,
    /// Print machine-readable JSON
    #[arg(long, short = 'j')]
    json: bool,
}

#[derive(Args)]
pub struct InitArgs {
    /// time project id (see `eimer time projects`)
    project_id: u64,
    /// Alias used with --project (defaults to a slug of the project name)
    #[arg(long)]
    alias: Option<String>,
}

#[derive(Args)]
pub struct PrArg {
    /// PR number (defaults to the current branch's PR)
    #[arg(long)]
    pr: Option<u64>,
}

#[derive(Args)]
pub struct SuggestArgs {
    #[command(flatten)]
    target: PrArg,
    /// Print machine-readable JSON
    #[arg(long, short = 'j')]
    json: bool,
}

#[derive(Args)]
pub struct BookArgs {
    /// Duration, e.g. 1.5h, 90m, 1h30m, 1:30
    duration: String,
    #[command(flatten)]
    target: PrArg,
    /// Project alias from .time.json (required when it lists several)
    #[arg(long, short = 'p')]
    project: Option<String>,
    /// Booking date (yyyy-mm-dd); defaults to today, or the record's date on updates
    #[arg(long)]
    date: Option<String>,
    /// Record description (defaults to "<repo>#<pr>: <title>")
    #[arg(long, short = 'd')]
    description: Option<String>,
    /// Book as on-site work
    #[arg(long)]
    on_site: bool,
}

#[derive(Args)]
pub struct NoneArgs {
    /// Why no time is booked
    #[arg(required = true, num_args = 1..)]
    reason: Vec<String>,
    #[command(flatten)]
    target: PrArg,
    /// Overwrite an existing booking line (the time record itself is kept)
    #[arg(long)]
    force: bool,
}

#[derive(Args)]
pub struct StatusArgs {
    #[command(flatten)]
    target: PrArg,
}

pub fn projects(args: ProjectsArgs) -> Result<()> {
    let projects: Vec<_> = TimeClient::create()?
        .projects()?
        .into_iter()
        .filter(|p| args.all || !p.is_completed)
        .collect();
    if args.json {
        return ui::print_json(&projects);
    }
    let rows = projects.iter().map(|p| {
        vec![
            p.id.to_string(),
            p.project_number.clone().unwrap_or_default(),
            p.name.clone(),
        ]
    });
    println!("{}", ui::render_table(["ID", "Number", "Name"], rows));
    Ok(())
}

pub fn init(args: InitArgs) -> Result<()> {
    let project = TimeClient::create()?
        .projects()?
        .into_iter()
        .find(|p| p.id == args.project_id)
        .ok_or_else(|| anyhow!("No time project with id {}", args.project_id))
        .hint("See `eimer time projects`.")?;
    let alias = args.alias.unwrap_or_else(|| slug(&project.name));
    let mut config = repo::read_config()?.unwrap_or_default();
    config
        .projects
        .retain(|p| p.id != project.id && p.alias != alias);
    config.projects.push(ProjectConfig {
        alias: alias.clone(),
        id: project.id,
        name: Some(project.name.clone()),
    });
    repo::save_config(&config)?;
    ui::print_success(&format!(
        "Added @{alias} → {} ({}) to {}",
        project.name,
        project.id,
        repo::CONFIG_FILE
    ));
    Ok(())
}

pub fn suggest(args: SuggestArgs) -> Result<()> {
    let s = suggestion::suggest(&repo::commit_times(args.target.pr)?);
    if args.json {
        return ui::print_json(&s);
    }
    ui::print_info(
        &format!(
            "Suggested: {} ({} commits, {} sessions)",
            duration::format(s.hours),
            s.commits,
            s.sessions
        ),
        Some("A suggestion only; the user decides the booked duration."),
    );
    Ok(())
}

pub fn book(args: BookArgs) -> Result<()> {
    let hours = duration::parse(&args.duration)?;
    if let Some(date) = &args.date {
        chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d")
            .map_err(|_| anyhow!("Invalid --date \"{date}\", expected yyyy-mm-dd"))?;
    }
    let config = repo::load_config()?;
    let pr = repo::pull_request(args.target.pr)?;
    let existing = block::find(&pr.body).and_then(|(_, entry)| entry);

    let booked_alias = match &existing {
        Some(TimeEntry::Booked { project_alias, .. }) => Some(project_alias.as_deref()),
        _ => None,
    };
    if let (Some(booked), Some(requested)) = (booked_alias, args.project.as_deref())
        && booked.is_some_and(|b| b != requested)
    {
        bail!(
            "PR is booked on @{}; changing projects is not supported. Fix it in time.",
            booked.unwrap_or_default()
        );
    }
    let project = config.resolve(args.project.as_deref().or(booked_alias.flatten()))?;
    let client = TimeClient::create()?;
    let mut input = TimeRecordInput {
        project_id: project.id,
        date: args
            .date
            .clone()
            .unwrap_or_else(|| chrono::Local::now().format("%Y-%m-%d").to_string()),
        description: args.description.unwrap_or_else(|| pr.description()),
        duration: duration::to_api(hours),
        on_site: args.on_site,
    };

    let record_id = match existing {
        Some(TimeEntry::Booked { record_id, .. }) => {
            let current = client.find_record(project.id, record_id)?;
            if current.as_ref().is_some_and(|r| r.invoiced) {
                bail!("time#{record_id} is already invoiced; leave it as is.");
            }
            if let (None, Some(current)) = (&args.date, &current) {
                input.date = current.date.clone();
            }
            client.update_record(record_id, &input)?;
            let before = current.map(|r| duration::format(duration::from_api(r.duration)));
            ui::print_success(&format!(
                "Updated time#{record_id}: {} → {}",
                before.unwrap_or_else(|| "?".into()),
                duration::format(hours)
            ));
            record_id
        }
        _ => {
            let id = client.create_record(&input)?;
            ui::print_success(&format!(
                "Booked {} on {} (time#{id})",
                duration::format(hours),
                project.name.as_deref().unwrap_or(&project.alias)
            ));
            id
        }
    };

    write_entry(
        &pr,
        &TimeEntry::Booked {
            hours,
            record_id,
            project_alias: alias_for(&config, project),
        },
    )
}

/// Only repos with several projects need the alias in the Time line.
fn alias_for(config: &TimeConfig, project: &ProjectConfig) -> Option<String> {
    (config.projects.len() > 1).then(|| project.alias.clone())
}

pub fn none(args: NoneArgs) -> Result<()> {
    let reason = args.reason.join(" ").trim().to_string();
    if reason.is_empty() || reason.contains(['(', ')', '\n']) {
        bail!("Give a short reason without parentheses or line breaks");
    }
    let pr = repo::pull_request(args.target.pr)?;
    if let Some((_, Some(TimeEntry::Booked { record_id, .. }))) = block::find(&pr.body)
        && !args.force
    {
        return Err(anyhow!("PR is already booked as time#{record_id}"))
            .hint("Delete the record in time, then rerun with --force.");
    }
    write_entry(&pr, &TimeEntry::None { reason })
}

fn write_entry(pr: &PullRequest, entry: &TimeEntry) -> Result<()> {
    repo::set_body(pr.number, &block::upsert(&pr.body, entry))?;
    ui::print_info(&block::format_line(entry), Some(&pr.url));
    Ok(())
}

pub fn status(args: StatusArgs) -> Result<()> {
    let pr = repo::pull_request(args.target.pr)?;
    match block::find(&pr.body) {
        Some((line, Some(_))) => {
            ui::print_success(&line);
            Ok(())
        }
        Some((line, None)) => {
            ui::print_warning(&format!("Invalid: {line}"), Some(STATUS_HINT));
            Err(SilentExit(1).into())
        }
        None => {
            ui::print_warning("No Time line", Some(STATUS_HINT));
            Err(SilentExit(1).into())
        }
    }
}

const STATUS_HINT: &str = "Run `eimer time book <duration>` or `eimer time none <reason>`.";

fn slug(name: &str) -> String {
    let mut slug = String::new();
    for c in name.to_lowercase().chars() {
        let c = match c {
            'ä' => "ae".into(),
            'ö' => "oe".into(),
            'ü' => "ue".into(),
            'ß' => "ss".into(),
            c if c.is_ascii_alphanumeric() => c.to_string(),
            _ => "-".into(),
        };
        if c == "-" && (slug.is_empty() || slug.ends_with('-')) {
            continue;
        }
        slug.push_str(&c);
    }
    let slug = slug.trim_end_matches('-').to_string();
    if slug.is_empty() {
        "project".into()
    } else {
        slug
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs_project_names() {
        assert_eq!(slug("Lauterbad GmbH – Website"), "lauterbad-gmbh-website");
        assert_eq!(slug("Bürokram"), "buerokram");
        assert_eq!(slug("!!!"), "project");
    }
}
