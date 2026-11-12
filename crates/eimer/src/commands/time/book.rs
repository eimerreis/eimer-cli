//! `time projects|init|suggest|add|pending|book|none|status`.

use std::collections::BTreeMap;

use anyhow::{Result, anyhow, bail};
use clap::Args;
use serde::Serialize;

use super::activity::{self, RepoIdentity};
use super::api::{TimeClient, TimeRecordInput};
use super::block::{self, TimeEntry};
use super::pending::{self, Booking, Entry, PendingFile};
use super::repo::{self, ProjectConfig};
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
    /// Start of the window: yyyy-mm-dd or RFC 3339 (defaults to the last `add`, else today)
    #[arg(long)]
    since: Option<String>,
    /// Pause that ends an active span, e.g. 15m
    #[arg(long, default_value = "15m")]
    idle: String,
    /// Print machine-readable JSON
    #[arg(long, short = 'j')]
    json: bool,
}

#[derive(Args)]
pub struct AddArgs {
    /// Duration, e.g. 1.5h, 90m, 1h30m, 1:30
    duration: String,
    /// What was done; merged into the time record description on booking
    #[arg(long, short = 'd')]
    description: Option<String>,
    /// Project alias from .time.json (required when it lists several)
    #[arg(long, short = 'p')]
    project: Option<String>,
    /// Work date (yyyy-mm-dd); defaults to today
    #[arg(long)]
    date: Option<String>,
}

#[derive(Args)]
pub struct PendingArgs {
    /// Print machine-readable JSON
    #[arg(long, short = 'j')]
    json: bool,
}

#[derive(Args)]
pub struct BookArgs {
    /// Show what would be booked without booking
    #[arg(long, short = 'n')]
    dry_run: bool,
    /// Book as on-site work
    #[arg(long)]
    on_site: bool,
}

#[derive(Args)]
pub struct NoneArgs {
    /// Why no time is recorded
    #[arg(required = true, num_args = 1..)]
    reason: Vec<String>,
    #[command(flatten)]
    target: PrArg,
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

#[derive(Serialize)]
struct Suggestion {
    hours: f64,
    since: String,
    active_minutes: i64,
    days: Vec<DayActivity>,
}

#[derive(Serialize)]
struct DayActivity {
    date: String,
    active_minutes: i64,
}

pub fn suggest(args: SuggestArgs) -> Result<()> {
    let repo = RepoIdentity::current()?;
    let idle = (duration::parse(&args.idle)? * 3600.0).round() as i64;
    let since = match &args.since {
        Some(value) => parse_since(value)?,
        None => activity::covered_until(&repo)?.unwrap_or_else(start_of_today),
    };
    let now = chrono::Local::now().timestamp();
    let spans = activity::spans(&activity::collect(&repo, since)?, idle);
    let active = activity::seconds_within(&spans, since, now);

    let mut days = Vec::new();
    let mut day = local_date(since);
    while day <= chrono::Local::now().date_naive() {
        let (from, to) = day_bounds(day);
        let seconds = activity::seconds_within(&spans, from.max(since), to.min(now));
        if seconds > 0 {
            days.push(DayActivity {
                date: day.format("%Y-%m-%d").to_string(),
                active_minutes: seconds / 60,
            });
        }
        day = day.succ_opt().expect("date in range");
    }
    let s = Suggestion {
        hours: suggestion::round_up(active),
        since: format_local(since),
        active_minutes: active / 60,
        days,
    };
    if args.json {
        return ui::print_json(&s);
    }
    for d in &s.days {
        println!("{}  {}", d.date, format_minutes(d.active_minutes));
    }
    ui::print_info(
        &format!(
            "Suggested: {} ({} agent activity since {})",
            duration::format(s.hours),
            format_minutes(s.active_minutes),
            s.since
        ),
        Some("A suggestion only; the user decides the recorded duration."),
    );
    Ok(())
}

pub fn add(args: AddArgs) -> Result<()> {
    let hours = duration::parse(&args.duration)?;
    let date = match args.date {
        Some(date) => {
            chrono::NaiveDate::parse_from_str(&date, "%Y-%m-%d")
                .map_err(|_| anyhow!("Invalid --date \"{date}\", expected yyyy-mm-dd"))?;
            date
        }
        None => chrono::Local::now().format("%Y-%m-%d").to_string(),
    };
    let description = args.description.unwrap_or_default();
    if description.contains("\n---") {
        bail!("The description must not contain a line starting with ---");
    }
    let config = repo::load_config()?;
    let project = config.resolve(args.project.as_deref())?;
    let entry = Entry {
        hours,
        date,
        project: (config.projects.len() > 1).then(|| project.alias.clone()),
        description,
    };
    let root = repo::root()?;
    let path = pending::write(&root, &entry)?;
    activity::set_covered_until(&RepoIdentity::current()?, chrono::Local::now().timestamp())?;
    let relative = path.strip_prefix(&root).unwrap_or(&path);
    ui::print_success(&format!(
        "Recorded {} on {} in {}",
        duration::format(hours),
        entry.date,
        relative.display()
    ));
    ui::print_info(
        "Commit the file with your work",
        Some("`eimer time book` books all pending entries later."),
    );
    Ok(())
}

#[derive(Serialize)]
struct PendingSummary<'a> {
    entries: Vec<PendingRow<'a>>,
    total_hours: f64,
    last_booking: Option<String>,
}

#[derive(Serialize)]
struct PendingRow<'a> {
    file: String,
    #[serde(flatten)]
    entry: &'a Entry,
}

pub fn pending(args: PendingArgs) -> Result<()> {
    let root = repo::root()?;
    let files = pending::list(&root)?;
    let last_booking = std::fs::read_to_string(root.join(pending::LEDGER))
        .ok()
        .and_then(|l| pending::last_booking(&l));
    let summary = PendingSummary {
        entries: files
            .iter()
            .map(|f| PendingRow {
                file: file_name(f),
                entry: &f.entry,
            })
            .collect(),
        total_hours: files.iter().map(|f| f.entry.hours).sum(),
        last_booking,
    };
    if args.json {
        return ui::print_json(&summary);
    }
    if !summary.entries.is_empty() {
        let rows = summary.entries.iter().map(|r| {
            vec![
                r.entry.date.clone(),
                duration::format(r.entry.hours),
                r.entry.project.clone().unwrap_or_default(),
                r.entry.description.clone(),
            ]
        });
        println!(
            "{}",
            ui::render_table(["Date", "Hours", "Project", "Description"], rows)
        );
    }
    ui::print_info(
        &format!(
            "Pending: {} in {} entries",
            duration::format(summary.total_hours),
            summary.entries.len()
        ),
        Some(&format!(
            "Last booking: {}",
            summary.last_booking.as_deref().unwrap_or("none yet")
        )),
    );
    Ok(())
}

/// Books pending entries as one time record per project and day, removes them and
/// appends the bookings to the ledger.
pub fn book(args: BookArgs) -> Result<()> {
    let root = repo::root()?;
    let files = pending::list(&root)?;
    if files.is_empty() {
        ui::print_info(
            "Nothing pending",
            Some("Record work with `eimer time add`."),
        );
        return Ok(());
    }
    let config = repo::load_config()?;
    let mut groups: BTreeMap<(String, u64), (&ProjectConfig, Vec<&PendingFile>)> = BTreeMap::new();
    for file in &files {
        let project = config
            .resolve(file.entry.project.as_deref())
            .map_err(|e| anyhow!("{}: {e}", file_name(file)))?;
        groups
            .entry((file.entry.date.clone(), project.id))
            .or_insert_with(|| (project, Vec::new()))
            .1
            .push(file);
    }

    let prefix = RepoIdentity::current()?.name().to_string();
    let planned: Vec<_> = groups
        .into_iter()
        .map(|((date, _), (project, files))| {
            let hours: f64 = files.iter().map(|f| f.entry.hours).sum();
            let descriptions: Vec<&str> =
                files.iter().map(|f| f.entry.description.as_str()).collect();
            let input = TimeRecordInput {
                project_id: project.id,
                date,
                description: pending::summarize(&prefix, &descriptions),
                duration: duration::to_api(hours),
                on_site: args.on_site,
            };
            (project, hours, input, files)
        })
        .collect();

    if args.dry_run {
        let rows = planned.iter().map(|(project, hours, input, _)| {
            vec![
                input.date.clone(),
                project_label(project),
                duration::format(*hours),
                input.description.clone(),
            ]
        });
        println!(
            "{}",
            ui::render_table(["Date", "Project", "Hours", "Description"], rows)
        );
        ui::print_info("Dry run; nothing booked", None);
        return Ok(());
    }

    let client = TimeClient::create()?;
    let mut bookings = Vec::new();
    let mut failure = None;
    for (project, hours, input, files) in planned {
        let id = match client.create_record(&input) {
            Ok(id) => id,
            Err(err) => {
                failure = Some(err);
                break;
            }
        };
        for file in files {
            std::fs::remove_file(&file.path).ok();
        }
        ui::print_success(&format!(
            "Booked {} on {} for {} (time#{id})",
            duration::format(hours),
            project_label(project),
            input.date
        ));
        bookings.push(Booking {
            date: input.date,
            project: project_label(project),
            hours,
            record_id: id,
            description: input.description,
        });
    }

    if !bookings.is_empty() {
        let ledger_path = root.join(pending::LEDGER);
        let existing = std::fs::read_to_string(&ledger_path).ok();
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        let ledger = pending::append_ledger(existing.as_deref(), &today, &bookings);
        std::fs::write(&ledger_path, ledger + "\n")
            .map_err(|e| anyhow!("Failed to write {}: {e}", pending::LEDGER))?;
        ui::print_info(
            &format!("Appended to {}", pending::LEDGER),
            Some("Commit .time/ and TIME.md in a PR."),
        );
    }
    match failure {
        Some(err) => {
            Err(err).hint("Booked records were removed from .time/; rerun to book the rest.")
        }
        None => Ok(()),
    }
}

pub fn none(args: NoneArgs) -> Result<()> {
    let reason = args.reason.join(" ").trim().to_string();
    if reason.is_empty() || reason.contains(['(', ')', '\n']) {
        bail!("Give a short reason without parentheses or line breaks");
    }
    let pr = repo::pull_request(args.target.pr)?;
    let entry = TimeEntry::None { reason };
    repo::set_body(pr.number, &block::upsert(&pr.body, &entry))?;
    ui::print_info(&block::format_line(&entry), Some(&pr.url));
    Ok(())
}

/// Mirrors the `time-check` workflow in eimerreis/freelance-ops.
pub fn status(args: StatusArgs) -> Result<()> {
    let pr = repo::pull_request(args.target.pr)?;
    let changes = repo::pr_files(pr.number)?;
    let added: Vec<_> = changes
        .iter()
        .filter(|f| f.status == "added" && repo::is_pending_entry(&f.filename))
        .collect();
    if !added.is_empty() {
        for f in added {
            ui::print_success(&format!("Time entry {}", f.filename));
        }
        return Ok(());
    }
    if !changes.is_empty() && changes.iter().all(|f| repo::is_booking_file(&f.filename)) {
        ui::print_success("Booking PR (only .time/, TIME.md and changesets)");
        return Ok(());
    }
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
            ui::print_warning("No time entry and no Time line", Some(STATUS_HINT));
            Err(SilentExit(1).into())
        }
    }
}

const STATUS_HINT: &str =
    "Run `eimer time add <duration>` and commit the entry, or `eimer time none <reason>`.";

fn file_name(file: &PendingFile) -> String {
    file.path
        .file_name()
        .map(|n| format!("{}/{}", pending::DIR, n.to_string_lossy()))
        .unwrap_or_default()
}

fn project_label(project: &ProjectConfig) -> String {
    project
        .name
        .clone()
        .unwrap_or_else(|| project.alias.clone())
}

fn parse_since(value: &str) -> Result<i64> {
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(value) {
        return Ok(dt.timestamp());
    }
    let date = chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map_err(|_| anyhow!("Invalid --since \"{value}\", expected yyyy-mm-dd or RFC 3339"))?;
    Ok(day_bounds(date).0)
}

fn start_of_today() -> i64 {
    day_bounds(chrono::Local::now().date_naive()).0
}

/// Local midnight of `day` and of the next day, as unix seconds.
fn day_bounds(day: chrono::NaiveDate) -> (i64, i64) {
    let midnight = |d: chrono::NaiveDate| {
        d.and_hms_opt(0, 0, 0)
            .and_then(|t| t.and_local_timezone(chrono::Local).earliest())
            .map_or(0, |t| t.timestamp())
    };
    (
        midnight(day),
        midnight(day.succ_opt().expect("date in range")),
    )
}

fn local_date(ts: i64) -> chrono::NaiveDate {
    chrono::DateTime::from_timestamp(ts, 0)
        .expect("valid timestamp")
        .with_timezone(&chrono::Local)
        .date_naive()
}

fn format_local(ts: i64) -> String {
    chrono::DateTime::from_timestamp(ts, 0)
        .expect("valid timestamp")
        .with_timezone(&chrono::Local)
        .format("%Y-%m-%d %H:%M")
        .to_string()
}

fn format_minutes(minutes: i64) -> String {
    format!("{}h{:02}m", minutes / 60, minutes % 60)
}

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

    #[test]
    fn parses_since() {
        assert_eq!(parse_since("2026-11-12T10:00:00Z").unwrap(), 1_794_477_600);
        assert!(parse_since("2026-11-12").is_ok());
        assert!(parse_since("yesterday").is_err());
        assert_eq!(format_minutes(85), "1h25m");
    }
}
