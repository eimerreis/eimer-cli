//! `eimer time report --project <id|name>`: the time report document (Zeitbuchungen) for one
//! project. Writes the data as YAML in the `time-report` format and renders it to PDF with
//! `eimer doc` (see `commands/doc`). Records without the invoiced flag ("Abgr.") in time count as
//! not billed: the report lists them but keeps them out of the billed totals.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use chrono::NaiveDate;
use serde::Serialize;

use super::api::{Project, TimeClient, TimeRecord};
use crate::commands::doc;
use crate::core::error::HintExt;
use crate::core::ui;

pub struct DocumentRequest<'a> {
    pub project: &'a str,
    pub from: Option<NaiveDate>,
    pub to: Option<NaiveDate>,
    pub output: Option<&'a str>,
    pub json: bool,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct TimeReportDocument {
    pub client: String,
    pub project: String,
    pub period: Period,
    pub entries: Vec<Entry>,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct Period {
    pub from: String,
    pub to: String,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct Entry {
    pub date: String,
    /// `onsite` or `remote`
    pub location: &'static str,
    pub hours: f64,
    pub description: String,
    /// Whether the client is charged for the entry: the record's invoiced flag ("Abgr.") in time.
    pub billed: bool,
}

pub fn run(request: DocumentRequest) -> Result<()> {
    let client = TimeClient::create()?;
    let document = ui::spin_unless(request.json, "Loading time records", || {
        let projects = client.projects()?;
        let project = find_project(&projects, request.project)?;
        let records = client.all_time_records(project.id)?;
        let customer = customer_name(&client, project);
        build_document(
            project,
            customer.as_deref(),
            &records,
            request.from,
            request.to,
        )
    })?;
    if request.json {
        return ui::print_json(&document);
    }

    let pdf = output_path(request.output, &document);
    let yaml = pdf.with_extension("yaml");
    std::fs::write(&yaml, to_yaml(&document))
        .with_context(|| format!("Failed to write {}", yaml.display()))?;

    doc::render::render(&doc::RenderOptions {
        data: &yaml,
        output: Some(&pdf),
        sender: None,
        watch: false,
        png: false,
    })
    .with_context(|| {
        format!(
            "Failed to render the time report; the data is in {}",
            yaml.display()
        )
    })
    .hint(format!(
        "Fix the YAML if needed, then `eimer doc render {}`.",
        yaml.display()
    ))?;
    let hours: f64 = document.entries.iter().map(|e| e.hours).sum();
    let unbilled: f64 = document
        .entries
        .iter()
        .filter(|e| !e.billed)
        .map(|e| e.hours)
        .sum();
    ui::print_success(&format!(
        "{} ({} entries, {} h, {} h not billed)",
        pdf.display(),
        document.entries.len(),
        round(hours),
        round(unbilled)
    ));
    Ok(())
}

/// Finds a project by id, exact name, or a unique name fragment (case-insensitive).
pub fn find_project<'a>(projects: &'a [Project], query: &str) -> Result<&'a Project> {
    if let Ok(id) = query.parse::<u64>()
        && let Some(project) = projects.iter().find(|p| p.id == id)
    {
        return Ok(project);
    }
    let needle = query.to_lowercase();
    if let Some(project) = projects.iter().find(|p| p.name.to_lowercase() == needle) {
        return Ok(project);
    }
    let matches: Vec<&Project> = projects
        .iter()
        .filter(|p| p.name.to_lowercase().contains(&needle))
        .collect();
    match matches.as_slice() {
        [project] => Ok(project),
        [] => Err(anyhow!("No time project matches \"{query}\""))
            .hint("List them with `eimer time projects --all`"),
        many => Err(anyhow!(
            "\"{query}\" matches several projects: {}",
            many.iter()
                .map(|p| format!("{} ({})", p.name, p.id))
                .collect::<Vec<_>>()
                .join(", ")
        ))
        .hint("Pass the project id or a longer name"),
    }
}

/// The billing customer's organization; `None` when the project has none or the lookup fails.
fn customer_name(client: &TimeClient, project: &Project) -> Option<String> {
    let id = project.customer_id_billing_service.as_deref()?;
    match client.customers() {
        Ok(customers) => customers
            .into_iter()
            .find(|c| c.id == id)
            .map(|c| c.organization),
        Err(err) => {
            ui::print_warning(
                &format!("Could not load customers ({err})"),
                Some("Using the project name as client"),
            );
            None
        }
    }
}

/// Records inside the window, sorted by date. Without --from/--to the window spans all records.
pub fn build_document(
    project: &Project,
    customer: Option<&str>,
    records: &[TimeRecord],
    from: Option<NaiveDate>,
    to: Option<NaiveDate>,
) -> Result<TimeReportDocument> {
    let mut dated: Vec<(NaiveDate, &TimeRecord)> = records
        .iter()
        .filter_map(|r| Some((record_date(&r.date)?, r)))
        .filter(|(date, _)| from.is_none_or(|f| *date >= f) && to.is_none_or(|t| *date <= t))
        .collect();
    dated.sort_by_key(|(date, r)| (*date, r.id));

    let (Some(first), Some(last)) = (dated.first(), dated.last()) else {
        bail!(
            "No time records for {} in {} – {}",
            project.name,
            from.map_or("the beginning".into(), |d| d.to_string()),
            to.map_or("today".into(), |d| d.to_string()),
        );
    };
    Ok(TimeReportDocument {
        client: customer.unwrap_or(&project.name).to_string(),
        project: project.name.clone(),
        period: Period {
            from: from.unwrap_or(first.0).to_string(),
            to: to.unwrap_or(last.0).to_string(),
        },
        entries: dated
            .iter()
            .map(|(date, r)| Entry {
                date: date.to_string(),
                location: if r.on_site { "onsite" } else { "remote" },
                hours: round(r.duration as f64 / 100.0),
                description: r.description.trim().to_string(),
                billed: r.invoiced,
            })
            .collect(),
    })
}

/// `--output`, or `zeitbuchungen-<client>-<from>-<to>.pdf` in the current directory.
pub fn output_path(output: Option<&str>, document: &TimeReportDocument) -> PathBuf {
    match output {
        Some(path) => Path::new(path).with_extension("pdf"),
        None => PathBuf::from(format!(
            "zeitbuchungen-{}-{}-{}.pdf",
            slug(&document.client),
            document.period.from,
            document.period.to
        )),
    }
}

/// YAML for `eimer doc render`. Strings are JSON-quoted, which YAML reads as double-quoted scalars.
pub fn to_yaml(document: &TimeReportDocument) -> String {
    let quote = |s: &str| serde_json::to_string(s).expect("string serializes");
    let mut out = format!(
        "type: time-report\nclient: {}\nproject: {}\nperiod: {{ from: {}, to: {} }}\nentries:\n",
        quote(&document.client),
        quote(&document.project),
        document.period.from,
        document.period.to,
    );
    for entry in &document.entries {
        out.push_str(&format!(
            "  - date: {}\n    location: {}\n    hours: {}\n    description: {}\n",
            entry.date,
            entry.location,
            entry.hours,
            quote(&entry.description)
        ));
        if !entry.billed {
            out.push_str("    billed: false\n");
        }
    }
    out
}

fn slug(value: &str) -> String {
    let mut out = String::new();
    for c in value.to_lowercase().chars() {
        match c {
            'ä' => out.push_str("ae"),
            'ö' => out.push_str("oe"),
            'ü' => out.push_str("ue"),
            'ß' => out.push_str("ss"),
            c if c.is_ascii_alphanumeric() => out.push(c),
            _ if !out.ends_with('-') && !out.is_empty() => out.push('-'),
            _ => {}
        }
    }
    out.trim_end_matches('-').to_string()
}

fn record_date(raw: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(raw.get(..10)?, "%Y-%m-%d").ok()
}

fn round(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(id: u64, name: &str) -> Project {
        Project {
            id,
            name: name.into(),
            project_number: None,
            is_completed: false,
            customer_id_billing_service: None,
        }
    }

    fn record(id: u64, date: &str, duration: i64, on_site: bool, description: &str) -> TimeRecord {
        TimeRecord {
            id,
            description: description.into(),
            duration,
            date: date.into(),
            invoiced: true,
            on_site,
        }
    }

    fn d(v: &str) -> NaiveDate {
        NaiveDate::parse_from_str(v, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn find_project_by_id_name_or_unique_fragment() {
        let projects = [project(7, "IT-Betreuung Baur"), project(9, "Relaunch Shop")];
        assert_eq!(find_project(&projects, "9").unwrap().id, 9);
        assert_eq!(find_project(&projects, "relaunch shop").unwrap().id, 9);
        assert_eq!(find_project(&projects, "baur").unwrap().id, 7);
        assert!(find_project(&projects, "nope").is_err());
    }

    #[test]
    fn find_project_rejects_ambiguous_fragments() {
        let projects = [project(1, "Shop A"), project(2, "Shop B")];
        let err = find_project(&projects, "shop").unwrap_err().to_string();
        assert_eq!(
            err,
            "\"shop\" matches several projects: Shop A (1), Shop B (2)"
        );
    }

    #[test]
    fn build_document_filters_sorts_and_maps_location() {
        let records = [
            record(
                3,
                "2026-07-30T00:00:00",
                100,
                true,
                " Einrichtung OneDrive\nLizenz klären ",
            ),
            record(1, "2026-03-26", 50, false, "TeamViewer-Sitzung"),
            record(2, "2026-02-01", 400, false, "outside the window"),
        ];
        let doc = build_document(
            &project(7, "IT-Betreuung"),
            Some("Beispiel GmbH"),
            &records,
            Some(d("2026-03-01")),
            None,
        )
        .unwrap();

        assert_eq!(doc.client, "Beispiel GmbH");
        assert_eq!(
            doc.period,
            Period {
                from: "2026-03-01".into(),
                to: "2026-07-30".into()
            }
        );
        assert_eq!(
            doc.entries,
            vec![
                Entry {
                    date: "2026-03-26".into(),
                    location: "remote",
                    hours: 0.5,
                    description: "TeamViewer-Sitzung".into(),
                    billed: true,
                },
                Entry {
                    date: "2026-07-30".into(),
                    location: "onsite",
                    hours: 1.0,
                    description: "Einrichtung OneDrive\nLizenz klären".into(),
                    billed: true,
                },
            ]
        );
    }

    #[test]
    fn build_document_uses_the_project_name_without_customer() {
        let records = [record(1, "2026-03-26", 50, false, "x")];
        let doc = build_document(&project(7, "IT-Betreuung"), None, &records, None, None).unwrap();
        assert_eq!(doc.client, "IT-Betreuung");
        assert_eq!(doc.period.from, "2026-03-26");
    }

    #[test]
    fn build_document_fails_without_records_in_window() {
        let records = [record(1, "2026-03-26", 50, false, "x")];
        let err = build_document(
            &project(7, "P"),
            None,
            &records,
            Some(d("2026-04-01")),
            None,
        )
        .unwrap_err()
        .to_string();
        assert_eq!(err, "No time records for P in 2026-04-01 – today");
    }

    #[test]
    fn to_yaml_writes_the_time_report_format() {
        let doc = TimeReportDocument {
            client: "Beispiel GmbH".into(),
            project: "IT-Betreuung".into(),
            period: Period {
                from: "2026-03-26".into(),
                to: "2026-07-30".into(),
            },
            entries: vec![Entry {
                date: "2026-07-30".into(),
                location: "onsite",
                hours: 1.5,
                description: "Einrichtung \"OneDrive\"\nLizenz".into(),
                billed: true,
            }],
        };
        assert_eq!(
            to_yaml(&doc),
            "type: time-report\nclient: \"Beispiel GmbH\"\nproject: \"IT-Betreuung\"\nperiod: { from: 2026-03-26, to: 2026-07-30 }\nentries:\n  - date: 2026-07-30\n    location: onsite\n    hours: 1.5\n    description: \"Einrichtung \\\"OneDrive\\\"\\nLizenz\"\n"
        );
    }

    #[test]
    fn build_document_marks_records_without_the_invoiced_flag_as_not_billed() {
        let mut goodwill = record(2, "2026-03-27", 100, false, "Kulanz");
        goodwill.invoiced = false;
        let records = [record(1, "2026-03-26", 50, false, "Abgerechnet"), goodwill];
        let doc = build_document(&project(7, "P"), None, &records, None, None).unwrap();
        assert_eq!(
            doc.entries.iter().map(|e| e.billed).collect::<Vec<_>>(),
            vec![true, false]
        );
    }

    #[test]
    fn to_yaml_marks_only_unbilled_entries() {
        let entry = |billed| Entry {
            date: "2026-07-30".into(),
            location: "remote",
            hours: 1.0,
            description: "x".into(),
            billed,
        };
        let doc = TimeReportDocument {
            client: "C".into(),
            project: "C".into(),
            period: Period {
                from: "2026-07-30".into(),
                to: "2026-07-30".into(),
            },
            entries: vec![entry(true), entry(false)],
        };
        assert_eq!(
            to_yaml(&doc),
            "type: time-report\nclient: \"C\"\nproject: \"C\"\nperiod: { from: 2026-07-30, to: 2026-07-30 }\nentries:\n  - date: 2026-07-30\n    location: remote\n    hours: 1\n    description: \"x\"\n  - date: 2026-07-30\n    location: remote\n    hours: 1\n    description: \"x\"\n    billed: false\n"
        );
    }

    #[test]
    fn output_path_defaults_to_a_slug_of_client_and_period() {
        let doc = TimeReportDocument {
            client: "Bäckerei Müller & Söhne".into(),
            project: "P".into(),
            period: Period {
                from: "2026-03-26".into(),
                to: "2026-11-02".into(),
            },
            entries: vec![],
        };
        assert_eq!(
            output_path(None, &doc),
            PathBuf::from("zeitbuchungen-baeckerei-mueller-soehne-2026-03-26-2026-11-02.pdf")
        );
        assert_eq!(
            output_path(Some("out/report"), &doc),
            PathBuf::from("out/report.pdf")
        );
    }
}
