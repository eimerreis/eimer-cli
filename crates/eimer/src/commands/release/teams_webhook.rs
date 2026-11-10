//! Posting changelogs to Microsoft Teams incoming webhooks (port of `teams-webhook.ts`).
//! Payloads are serialized with the same key order as the TS `JSON.stringify` output.

use anyhow::{Result, anyhow, bail};
use serde::Serialize;

use super::changelog_format::{range_label, short};

pub struct TeamsPostInput<'a> {
    pub webhook_url: &'a str,
    pub pipeline_name: &'a str,
    pub from_run_id: Option<u64>,
    pub to_run_id: Option<u64>,
    pub from_commit: &'a str,
    pub to_commit: &'a str,
    pub area: &'a str,
    pub pipeline_run_url: Option<String>,
    pub markdown: &'a str,
}

/// Tries an Adaptive Card first, then falls back to a MessageCard.
/// Returns the format that was accepted (`adaptive-card` / `message-card`).
pub fn post_changelog_to_teams(input: &TeamsPostInput) -> Result<&'static str> {
    let http = crate::core::http::client();
    let adaptive = serde_json::to_string(&build_adaptive_card_payload(input))?;
    let Some(adaptive_failure) = try_post(&http, input.webhook_url, adaptive)? else {
        return Ok("adaptive-card");
    };
    let message_card = serde_json::to_string(&build_message_card_payload(input))?;
    let Some(message_card_failure) = try_post(&http, input.webhook_url, message_card)? else {
        return Ok("message-card");
    };
    bail!(
        "Failed to post to Teams webhook. Adaptive card error: {adaptive_failure}. Message card error: {message_card_failure}."
    )
}

fn try_post(
    http: &reqwest::blocking::Client,
    webhook_url: &str,
    body: String,
) -> Result<Option<String>> {
    let response = http
        .post(webhook_url)
        .header("Content-Type", "application/json")
        .body(body)
        .send()
        .map_err(|err| anyhow!("Failed to reach Teams webhook: {err}"))?;
    let status = response.status();
    if status.is_success() {
        return Ok(None);
    }
    let body = response.text().unwrap_or_default();
    let body = body.trim();
    Ok(Some(format!(
        "HTTP {} {}: {}",
        status.as_u16(),
        status.canonical_reason().unwrap_or_default(),
        if body.is_empty() {
            "(empty response)"
        } else {
            body
        }
    )))
}

// ---------------------------------------------------------------------------
// Payload types

#[derive(Serialize)]
pub struct Fact {
    title: &'static str,
    value: String,
}

#[derive(Serialize)]
pub struct AdaptiveMessage {
    #[serde(rename = "type")]
    kind: &'static str,
    attachments: Vec<AdaptiveAttachment>,
}

#[derive(Serialize)]
struct AdaptiveAttachment {
    #[serde(rename = "contentType")]
    content_type: &'static str,
    content: AdaptiveCard,
}

#[derive(Serialize)]
struct AdaptiveCard {
    #[serde(rename = "$schema")]
    schema: &'static str,
    #[serde(rename = "type")]
    kind: &'static str,
    version: &'static str,
    msteams: MsTeams,
    body: Vec<Element>,
    #[serde(skip_serializing_if = "Option::is_none")]
    actions: Option<Vec<OpenUrlAction>>,
}

#[derive(Serialize)]
struct MsTeams {
    width: &'static str,
}

#[derive(Serialize)]
struct OpenUrlAction {
    #[serde(rename = "type")]
    kind: &'static str,
    title: &'static str,
    url: String,
}

#[derive(Serialize)]
#[serde(tag = "type")]
enum Element {
    ColumnSet {
        columns: Vec<Element>,
    },
    Column {
        width: &'static str,
        items: Vec<Element>,
        #[serde(
            rename = "verticalContentAlignment",
            skip_serializing_if = "Option::is_none"
        )]
        vertical_content_alignment: Option<&'static str>,
    },
    TextBlock {
        text: String,
        wrap: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        size: Option<&'static str>,
        #[serde(skip_serializing_if = "Option::is_none")]
        weight: Option<&'static str>,
        #[serde(skip_serializing_if = "Option::is_none")]
        spacing: Option<&'static str>,
    },
    Badge {
        text: &'static str,
        size: &'static str,
        style: &'static str,
    },
    /// Field order differs between the TS containers, so it is captured by variant.
    Container(ContainerBody),
    FactSet {
        facts: Vec<Fact>,
    },
}

#[derive(Serialize)]
#[serde(untagged)]
enum ContainerBody {
    /// `{ separator, spacing, items }`
    Facts {
        separator: bool,
        spacing: &'static str,
        items: Vec<Element>,
    },
    /// `{ separator, spacing, style, items }`
    Section {
        separator: bool,
        spacing: &'static str,
        style: &'static str,
        items: Vec<Element>,
    },
    /// `{ separator, style, spacing, items }`
    Fallback {
        separator: bool,
        style: &'static str,
        spacing: &'static str,
        items: Vec<Element>,
    },
}

fn text_block(
    text: String,
    size: Option<&'static str>,
    weight: Option<&'static str>,
    spacing: Option<&'static str>,
) -> Element {
    Element::TextBlock {
        text,
        wrap: true,
        size,
        weight,
        spacing,
    }
}

#[derive(Serialize)]
pub struct MessageCard {
    #[serde(rename = "@type")]
    kind: &'static str,
    #[serde(rename = "@context")]
    context: &'static str,
    summary: &'static str,
    #[serde(rename = "themeColor")]
    theme_color: &'static str,
    title: &'static str,
    sections: (MessageFactsSection, MessageTextSection),
    #[serde(rename = "potentialAction", skip_serializing_if = "Option::is_none")]
    potential_action: Option<Vec<OpenUri>>,
}

#[derive(Serialize)]
struct MessageFactsSection {
    markdown: bool,
    facts: Vec<Fact>,
}

#[derive(Serialize)]
struct MessageTextSection {
    markdown: bool,
    text: String,
}

#[derive(Serialize)]
struct OpenUri {
    #[serde(rename = "@type")]
    kind: &'static str,
    name: &'static str,
    targets: Vec<UriTarget>,
}

#[derive(Serialize)]
struct UriTarget {
    os: &'static str,
    uri: String,
}

// ---------------------------------------------------------------------------
// Builders

pub fn build_adaptive_card_payload(input: &TeamsPostInput) -> AdaptiveMessage {
    AdaptiveMessage {
        kind: "message",
        attachments: vec![AdaptiveAttachment {
            content_type: "application/vnd.microsoft.card.adaptive",
            content: AdaptiveCard {
                schema: "https://adaptivecards.io/schemas/adaptive-card.json",
                kind: "AdaptiveCard",
                version: "1.4",
                msteams: MsTeams { width: "Full" },
                body: build_adaptive_body(input),
                actions: input.pipeline_run_url.as_ref().map(|url| {
                    vec![OpenUrlAction {
                        kind: "Action.OpenUrl",
                        title: "Open Pipeline Run",
                        url: url.clone(),
                    }]
                }),
            },
        }],
    }
}

pub fn build_message_card_payload(input: &TeamsPostInput) -> MessageCard {
    MessageCard {
        kind: "MessageCard",
        context: "https://schema.org/extensions",
        summary: "Release changelog",
        theme_color: "0078D4",
        title: "Release Changelog",
        sections: (
            MessageFactsSection {
                markdown: true,
                facts: build_facts(input),
            },
            MessageTextSection {
                markdown: true,
                text: input.markdown.to_string(),
            },
        ),
        potential_action: input.pipeline_run_url.as_ref().map(|url| {
            vec![OpenUri {
                kind: "OpenUri",
                name: "Open Pipeline Run",
                targets: vec![UriTarget {
                    os: "default",
                    uri: url.clone(),
                }],
            }]
        }),
    }
}

fn build_facts(input: &TeamsPostInput) -> Vec<Fact> {
    let mut facts = vec![
        Fact {
            title: "Pipeline",
            value: input.pipeline_name.to_string(),
        },
        Fact {
            title: "Range",
            value: format!(
                "{} -> {}",
                range_label(input.from_run_id, input.from_commit),
                range_label(input.to_run_id, input.to_commit)
            ),
        },
        Fact {
            title: "Commits",
            value: format!("{}..{}", short(input.from_commit), short(input.to_commit)),
        },
    ];
    if !input.area.is_empty() {
        facts.push(Fact {
            title: "Area",
            value: input.area.to_string(),
        });
    }
    facts
}

fn build_adaptive_body(input: &TeamsPostInput) -> Vec<Element> {
    let sections = parse_markdown_sections(input.markdown);
    let mut body = vec![
        Element::ColumnSet {
            columns: vec![
                Element::Column {
                    width: "stretch",
                    items: vec![text_block(
                        format_pipeline_display_title(input.pipeline_name),
                        Some("ExtraLarge"),
                        Some("Bolder"),
                        None,
                    )],
                    vertical_content_alignment: None,
                },
                Element::Column {
                    width: "auto",
                    items: vec![Element::Badge {
                        text: "Release",
                        size: "Large",
                        style: "Accent",
                    }],
                    vertical_content_alignment: Some("Center"),
                },
            ],
        },
        Element::Container(ContainerBody::Facts {
            separator: true,
            spacing: "Medium",
            items: vec![Element::FactSet {
                facts: build_facts(input),
            }],
        }),
    ];

    for (index, section) in sections.iter().enumerate() {
        body.push(Element::Container(ContainerBody::Section {
            separator: index == 0,
            spacing: if index == 0 { "Large" } else { "Medium" },
            style: "emphasis",
            items: vec![
                text_block(section.0.clone(), Some("Large"), Some("Bolder"), None),
                text_block(section.1.join("\n"), None, None, Some("Medium")),
            ],
        }));
    }

    if sections.is_empty() {
        body.push(Element::Container(ContainerBody::Fallback {
            separator: true,
            style: "emphasis",
            spacing: "Large",
            items: vec![text_block(input.markdown.to_string(), None, None, None)],
        }));
    }
    body
}

/// Strips a leading `tapio`, `tapio.` or `tapio-` (case-insensitive).
fn format_pipeline_display_title(pipeline_name: &str) -> String {
    let stripped = match pipeline_name.get(..5) {
        Some(prefix) if prefix.eq_ignore_ascii_case("tapio") => {
            let rest = &pipeline_name[5..];
            rest.strip_prefix(['.', '-']).unwrap_or(rest)
        }
        _ => pipeline_name,
    };
    let trimmed = stripped.trim();
    if trimmed.is_empty() {
        pipeline_name.to_string()
    } else {
        trimmed.to_string()
    }
}

/// `### Title` sections with their non-empty lines; sections without items are dropped.
fn parse_markdown_sections(markdown: &str) -> Vec<(String, Vec<String>)> {
    let mut sections: Vec<(String, Vec<String>)> = Vec::new();
    for line in markdown.split('\n') {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some(title) = trimmed.strip_prefix("### ") {
            sections.push((title.trim().to_string(), Vec::new()));
            continue;
        }
        if let Some(current) = sections.last_mut() {
            current.1.push(trimmed.to_string());
        }
    }
    sections.retain(|(_, items)| !items.is_empty());
    sections
}

#[cfg(test)]
mod tests {
    use super::super::test_server::{Reply, serve};
    use super::*;

    const MARKDOWN: &str = "## Release Changelog\n\nPipeline: tapio-app\nBranch: master\nRange: run #100 -> run #120 | area: frontend\nCommits: aaaaaaa..bbbbbbb\n\n### Features\n- feat(ui): new \"button\" ([PR 12](https://x/pr/12))\n- feat: ümlaut (`1234567`)\n\n### Empty\n\n### Fixes\n- fix: crash (`2222222`)";

    fn input(
        url: Option<&str>,
        area: &'static str,
        markdown: &'static str,
    ) -> TeamsPostInput<'static> {
        TeamsPostInput {
            webhook_url: "",
            pipeline_name: "tapio-app",
            from_run_id: Some(100),
            to_run_id: url.map(|_| 120),
            from_commit: "aaaaaaaaaa",
            to_commit: "bbbbbbbbbb",
            area,
            pipeline_run_url: url.map(String::from),
            markdown,
        }
    }

    // Expected strings were produced by the TS `postChangelogToTeams` (captured request bodies)
    // for the same inputs.
    const TS_ADAPTIVE: &str = r#"{"type":"message","attachments":[{"contentType":"application/vnd.microsoft.card.adaptive","content":{"$schema":"https://adaptivecards.io/schemas/adaptive-card.json","type":"AdaptiveCard","version":"1.4","msteams":{"width":"Full"},"body":[{"type":"ColumnSet","columns":[{"type":"Column","width":"stretch","items":[{"type":"TextBlock","text":"app","wrap":true,"size":"ExtraLarge","weight":"Bolder"}]},{"type":"Column","width":"auto","items":[{"type":"Badge","text":"Release","size":"Large","style":"Accent"}],"verticalContentAlignment":"Center"}]},{"type":"Container","separator":true,"spacing":"Medium","items":[{"type":"FactSet","facts":[{"title":"Pipeline","value":"tapio-app"},{"title":"Range","value":"run #100 -> run #120"},{"title":"Commits","value":"aaaaaaa..bbbbbbb"},{"title":"Area","value":"frontend"}]}]},{"type":"Container","separator":true,"spacing":"Large","style":"emphasis","items":[{"type":"TextBlock","text":"Features","wrap":true,"size":"Large","weight":"Bolder"},{"type":"TextBlock","text":"- feat(ui): new \"button\" ([PR 12](https://x/pr/12))\n- feat: ümlaut (`1234567`)","wrap":true,"spacing":"Medium"}]},{"type":"Container","separator":false,"spacing":"Medium","style":"emphasis","items":[{"type":"TextBlock","text":"Fixes","wrap":true,"size":"Large","weight":"Bolder"},{"type":"TextBlock","text":"- fix: crash (`2222222`)","wrap":true,"spacing":"Medium"}]}],"actions":[{"type":"Action.OpenUrl","title":"Open Pipeline Run","url":"https://dev.azure.com/o/p/_build/results?buildId=120"}]}}]}"#;
    const TS_MESSAGE_CARD: &str = r#"{"@type":"MessageCard","@context":"https://schema.org/extensions","summary":"Release changelog","themeColor":"0078D4","title":"Release Changelog","sections":[{"markdown":true,"facts":[{"title":"Pipeline","value":"tapio-app"},{"title":"Range","value":"run #100 -> bbbbbbb"},{"title":"Commits","value":"aaaaaaa..bbbbbbb"}]},{"markdown":true,"text":"no sections here"}]}"#;
    const TS_MESSAGE_CARD_FULL: &str = r###"{"@type":"MessageCard","@context":"https://schema.org/extensions","summary":"Release changelog","themeColor":"0078D4","title":"Release Changelog","sections":[{"markdown":true,"facts":[{"title":"Pipeline","value":"tapio-app"},{"title":"Range","value":"run #100 -> run #120"},{"title":"Commits","value":"aaaaaaa..bbbbbbb"},{"title":"Area","value":"frontend"}]},{"markdown":true,"text":"## Release Changelog\n\nPipeline: tapio-app\nBranch: master\nRange: run #100 -> run #120 | area: frontend\nCommits: aaaaaaa..bbbbbbb\n\n### Features\n- feat(ui): new \"button\" ([PR 12](https://x/pr/12))\n- feat: ümlaut (`1234567`)\n\n### Empty\n\n### Fixes\n- fix: crash (`2222222`)"}],"potentialAction":[{"@type":"OpenUri","name":"Open Pipeline Run","targets":[{"os":"default","uri":"https://dev.azure.com/o/p/_build/results?buildId=120"}]}]}"###;
    const TS_ADAPTIVE_FALLBACK: &str = r#"{"type":"message","attachments":[{"contentType":"application/vnd.microsoft.card.adaptive","content":{"$schema":"https://adaptivecards.io/schemas/adaptive-card.json","type":"AdaptiveCard","version":"1.4","msteams":{"width":"Full"},"body":[{"type":"ColumnSet","columns":[{"type":"Column","width":"stretch","items":[{"type":"TextBlock","text":"app","wrap":true,"size":"ExtraLarge","weight":"Bolder"}]},{"type":"Column","width":"auto","items":[{"type":"Badge","text":"Release","size":"Large","style":"Accent"}],"verticalContentAlignment":"Center"}]},{"type":"Container","separator":true,"spacing":"Medium","items":[{"type":"FactSet","facts":[{"title":"Pipeline","value":"tapio-app"},{"title":"Range","value":"run #100 -> bbbbbbb"},{"title":"Commits","value":"aaaaaaa..bbbbbbb"}]}]},{"type":"Container","separator":true,"style":"emphasis","spacing":"Large","items":[{"type":"TextBlock","text":"no sections here","wrap":true}]}]}}]}"#;

    #[test]
    fn adaptive_card_matches_ts() {
        let i = input(
            Some("https://dev.azure.com/o/p/_build/results?buildId=120"),
            "frontend",
            MARKDOWN,
        );
        let json = serde_json::to_string(&build_adaptive_card_payload(&i)).unwrap();
        assert_eq!(json, TS_ADAPTIVE);
        let json = serde_json::to_string(&build_message_card_payload(&i)).unwrap();
        assert_eq!(json, TS_MESSAGE_CARD_FULL);
    }

    #[test]
    fn fallback_payloads_match_ts() {
        let i = input(None, "", "no sections here");
        assert_eq!(
            serde_json::to_string(&build_adaptive_card_payload(&i)).unwrap(),
            TS_ADAPTIVE_FALLBACK
        );
        assert_eq!(
            serde_json::to_string(&build_message_card_payload(&i)).unwrap(),
            TS_MESSAGE_CARD
        );
    }

    #[test]
    fn display_title() {
        assert_eq!(format_pipeline_display_title("tapio-app"), "app");
        assert_eq!(format_pipeline_display_title("Tapio.Portal "), "Portal");
        assert_eq!(format_pipeline_display_title("tapioapp"), "app");
        assert_eq!(format_pipeline_display_title("tapio-"), "tapio-");
        assert_eq!(format_pipeline_display_title("other"), "other");
    }

    #[test]
    fn posts_with_message_card_fallback() {
        let (base, log) = serve(2, |req| {
            if req.body.contains("AdaptiveCard") {
                Reply {
                    status: 400,
                    headers: vec![],
                    body: "bad card".into(),
                }
            } else {
                Reply {
                    status: 200,
                    headers: vec![],
                    body: "1".into(),
                }
            }
        });
        let url = format!("{base}/hook");
        let mut i = input(None, "", "no sections here");
        i.webhook_url = &url;
        assert_eq!(post_changelog_to_teams(&i).unwrap(), "message-card");
        let log = log.lock().unwrap();
        assert_eq!(log[0].body, TS_ADAPTIVE_FALLBACK);
        assert_eq!(log[1].body, TS_MESSAGE_CARD);
        assert_eq!(log[1].method, "POST");
        assert!(
            log[1]
                .headers
                .iter()
                .any(|(k, v)| k == "content-type" && v == "application/json")
        );
    }

    #[test]
    fn reports_both_failures() {
        let (base, _) = serve(2, |_| Reply {
            status: 500,
            headers: vec![],
            body: String::new(),
        });
        let url = format!("{base}/hook");
        let mut i = input(None, "", "x");
        i.webhook_url = &url;
        let err = post_changelog_to_teams(&i).unwrap_err().to_string();
        assert_eq!(
            err,
            "Failed to post to Teams webhook. Adaptive card error: HTTP 500 Internal Server Error: (empty response). Message card error: HTTP 500 Internal Server Error: (empty response)."
        );
    }
}
