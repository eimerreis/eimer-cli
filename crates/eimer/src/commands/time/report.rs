//! `eimer time report`: hours per project from codeunity time, for dashboards and scripts.
//!
//! Per project: hours in a date window (default: the current ISO week), hours not yet
//! invoiced (with the oldest unbilled day) and budget progress where time tracks one.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Result, anyhow};
use chrono::{Datelike, Duration, Local, NaiveDate};
use clap::Args;
use serde::Serialize;

use super::api::{TimeClient, TimeRecord};
use crate::core::ui;

#[derive(Args)]
pub struct ReportArgs {
    /// Start of the window, yyyy-mm-dd (defaults to Monday of the current week)
    #[arg(long)]
    from: Option<String>,
    /// End of the window, yyyy-mm-dd, inclusive (defaults to today)
    #[arg(long)]
    to: Option<String>,
    /// Print machine-readable JSON
    #[arg(long, short = 'j')]
    json: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub from: String,
    pub to: String,
    pub projects: Vec<ProjectReport>,
    pub totals: Totals,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Totals {
    pub period_hours: f64,
    pub unbilled_hours: f64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectReport {
    pub id: u64,
    pub name: String,
    pub project_number: Option<String>,
    pub completed: bool,
    /// Hours inside the window, total and per day.
    pub period_hours: f64,
    pub days: Vec<DayHours>,
    /// Hours not yet invoiced, regardless of the window.
    pub unbilled_hours: f64,
    pub oldest_unbilled_date: Option<String>,
    pub budget: Option<Budget>,
}

#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DayHours {
    pub date: String,
    pub hours: f64,
}

#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Budget {
    /// Budgeted hours; absent when time only tracks completion.
    pub hours: Option<f64>,
    pub completed_hours: f64,
}

pub fn report(args: ReportArgs) -> Result<()> {
    let (from, to) = window(
        args.from.as_deref(),
        args.to.as_deref(),
        Local::now().date_naive(),
    )?;
    let client = TimeClient::create()?;
    let report = ui::spin_unless(args.json, "Loading time records", || {
        build(&client, from, to)
    })?;
    if args.json {
        return ui::print_json(&report);
    }
    print_human(&report);
    Ok(())
}

fn build(client: &TimeClient, from: NaiveDate, to: NaiveDate) -> Result<Report> {
    let projects = client.projects()?;
    let unbilled: BTreeSet<u64> = client.unbilled_project_ids()?.into_iter().collect();
    let budgets: BTreeMap<u64, Budget> = client
        .project_overview()?
        .into_iter()
        .filter_map(|p| Some((p.id, parse_budget(p.progress.as_ref()?)?)))
        .collect();

    let mut out = Vec::new();
    for project in projects
        .into_iter()
        .filter(|p| !p.is_completed || unbilled.contains(&p.id))
    {
        let records = client.all_time_records(project.id)?;
        let summary = summarize(&records, from, to);
        out.push(ProjectReport {
            id: project.id,
            budget: budgets.get(&project.id).map(|b| Budget {
                hours: b.hours,
                completed_hours: b.completed_hours,
            }),
            name: project.name,
            project_number: project.project_number,
            completed: project.is_completed,
            period_hours: summary.period_hours,
            days: summary.days,
            unbilled_hours: summary.unbilled_hours,
            oldest_unbilled_date: summary.oldest_unbilled_date,
        });
    }
    out.sort_by(|a, b| {
        b.unbilled_hours
            .total_cmp(&a.unbilled_hours)
            .then(b.period_hours.total_cmp(&a.period_hours))
    });
    let totals = Totals {
        period_hours: round(out.iter().map(|p| p.period_hours).sum()),
        unbilled_hours: round(out.iter().map(|p| p.unbilled_hours).sum()),
    };
    Ok(Report {
        from: from.to_string(),
        to: to.to_string(),
        projects: out,
        totals,
    })
}

struct Summary {
    period_hours: f64,
    days: Vec<DayHours>,
    unbilled_hours: f64,
    oldest_unbilled_date: Option<String>,
}

fn summarize(records: &[TimeRecord], from: NaiveDate, to: NaiveDate) -> Summary {
    let mut days: BTreeMap<NaiveDate, i64> = BTreeMap::new();
    let (mut unbilled, mut oldest) = (0_i64, None::<NaiveDate>);
    for record in records {
        let Some(date) = record_date(&record.date) else {
            continue;
        };
        if date >= from && date <= to {
            *days.entry(date).or_default() += record.duration;
        }
        if !record.invoiced {
            unbilled += record.duration;
            oldest = Some(oldest.map_or(date, |o| o.min(date)));
        }
    }
    let period: i64 = days.values().sum();
    Summary {
        period_hours: hours(period),
        days: days
            .into_iter()
            .map(|(date, d)| DayHours {
                date: date.to_string(),
                hours: hours(d),
            })
            .collect(),
        unbilled_hours: hours(unbilled),
        oldest_unbilled_date: oldest.map(|d| d.to_string()),
    }
}

/// Reads `FullProgress` (budget hours + completed) or `OnlyCompletion` (completed only).
fn parse_budget(progress: &serde_json::Value) -> Option<Budget> {
    let completed = progress.get("completedHours")?.as_f64()?;
    let hours = progress.get("duration").and_then(serde_json::Value::as_f64);
    Some(Budget {
        hours,
        completed_hours: completed,
    })
}

fn window(
    from: Option<&str>,
    to: Option<&str>,
    today: NaiveDate,
) -> Result<(NaiveDate, NaiveDate)> {
    let parse = |v: &str| {
        NaiveDate::parse_from_str(v, "%Y-%m-%d")
            .map_err(|_| anyhow!("Invalid date \"{v}\", expected yyyy-mm-dd"))
    };
    let to = to.map(parse).transpose()?.unwrap_or(today);
    let from = match from {
        Some(v) => parse(v)?,
        None => to - Duration::days(i64::from(to.weekday().num_days_from_monday())),
    };
    if from > to {
        return Err(anyhow!("--from {from} is after --to {to}"));
    }
    Ok((from, to))
}

fn record_date(raw: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(raw.get(..10)?, "%Y-%m-%d").ok()
}

fn hours(hundredths: i64) -> f64 {
    round(hundredths as f64 / 100.0)
}

fn round(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

fn print_human(report: &Report) {
    println!("{}", ui::bold(&format!("{} – {}", report.from, report.to)));
    let rows = report
        .projects
        .iter()
        .filter(|p| p.period_hours > 0.0 || p.unbilled_hours > 0.0)
        .map(|p| {
            vec![
                p.name.clone(),
                format!("{:.2}", p.period_hours),
                format!("{:.2}", p.unbilled_hours),
                p.oldest_unbilled_date.clone().unwrap_or_default(),
                p.budget
                    .as_ref()
                    .map(|b| match b.hours {
                        Some(h) => format!("{:.0} / {:.0} h", b.completed_hours, h),
                        None => format!("{:.0} h", b.completed_hours),
                    })
                    .unwrap_or_default(),
            ]
        });
    println!(
        "{}",
        ui::render_table(
            [
                "Project",
                "Period h",
                "Unbilled h",
                "Oldest unbilled",
                "Budget"
            ],
            rows
        )
    );
    println!(
        "{}",
        ui::dim(&format!(
            "Total: {:.2} h in period, {:.2} h unbilled",
            report.totals.period_hours, report.totals.unbilled_hours
        ))
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(date: &str, duration: i64, invoiced: bool) -> TimeRecord {
        TimeRecord {
            id: 1,
            description: String::new(),
            duration,
            date: date.into(),
            invoiced,
        }
    }

    fn d(v: &str) -> NaiveDate {
        NaiveDate::parse_from_str(v, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn window_defaults_to_current_week() {
        let (from, to) = window(None, None, d("2026-11-12")).unwrap();
        assert_eq!((from, to), (d("2026-11-09"), d("2026-11-12")));
    }

    #[test]
    fn window_rejects_inverted_range() {
        assert!(window(Some("2026-11-13"), Some("2026-11-12"), d("2026-11-12")).is_err());
    }

    #[test]
    fn summarize_splits_period_and_unbilled() {
        let records = [
            record("2026-10-20T00:00:00", 200, false),
            record("2026-11-10", 150, true),
            record("2026-11-11", 50, false),
            record("2026-11-11", 100, false),
        ];
        let s = summarize(&records, d("2026-11-09"), d("2026-11-15"));
        assert_eq!(s.period_hours, 3.0);
        assert_eq!(
            s.days,
            vec![
                DayHours {
                    date: "2026-11-10".into(),
                    hours: 1.5
                },
                DayHours {
                    date: "2026-11-11".into(),
                    hours: 1.5
                },
            ]
        );
        assert_eq!(s.unbilled_hours, 3.5);
        assert_eq!(s.oldest_unbilled_date.as_deref(), Some("2026-10-20"));
    }

    #[test]
    fn parse_budget_reads_both_progress_shapes() {
        let full = serde_json::json!({"$type": "FullProgress", "duration": 80, "completedHours": 62, "percent": "77"});
        let only = serde_json::json!({"$type": "OnlyCompletion", "completedHours": 12});
        assert_eq!(
            parse_budget(&full),
            Some(Budget {
                hours: Some(80.0),
                completed_hours: 62.0
            })
        );
        assert_eq!(
            parse_budget(&only),
            Some(Budget {
                hours: None,
                completed_hours: 12.0
            })
        );
    }
}
