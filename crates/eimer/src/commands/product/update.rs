//! `eimer product update`: pick the changes of one feature, write a draft post and copy
//! the prompt that lets a coding agent finish it.

use std::collections::BTreeSet;

use anyhow::{Context, Result, anyhow, bail};
use clap::{Args, ValueEnum};
use serde::Serialize;

use super::changes::{self, Change, SystemGit};
use super::config::{self, Repo, is_iso_date};
use super::frontmatter::{Document, Value};
use super::posts::{self, IgnoredChange};
use super::template::{self, PromptInput};
use crate::core::error::HintExt;
use crate::core::process::{Copied, NoClipboard, copy_to_clipboard};
use crate::core::{prompt, ui};

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Media {
    /// Let the agent decide (screenshot unless motion is the point)
    Auto,
    /// Real screenshots from UI tests
    Screenshot,
    /// Animated demo (Stageshow)
    Demo,
    /// No media
    None,
}

impl Media {
    fn as_str(self) -> &'static str {
        match self {
            Media::Auto => "auto",
            Media::Screenshot => "screenshot",
            Media::Demo => "demo",
            Media::None => "none",
        }
    }
}

#[derive(Args)]
pub struct UpdateArgs {
    /// Change ids to include, comma-separated (`all` = every user-facing change). Skips the picker and all questions
    #[arg(long, value_delimiter = ',')]
    select: Vec<String>,
    /// Post title (defaults to the first change)
    #[arg(long)]
    title: Option<String>,
    /// Media the agent should produce
    #[arg(long, value_enum)]
    media: Option<Media>,
    /// Also refresh the App Store screenshots
    #[arg(long)]
    store: bool,
    /// Hide the unchecked internal changes (ci, chore, excluded products) for good
    #[arg(long)]
    ignore_internal: bool,
    /// Post date, YYYY-MM-DD (default: today)
    #[arg(long)]
    date: Option<String>,
    /// Print the prompt instead of copying it
    #[arg(long, short = 'p')]
    print: bool,
    /// Write the prompt to a file instead of copying it
    #[arg(long, short = 'o', value_name = "FILE")]
    out: Option<String>,
    /// Print machine-readable JSON (draft path, changes and prompt)
    #[arg(long, short = 'j')]
    json: bool,
}

#[derive(Serialize)]
struct UpdateOutput<'a> {
    draft: &'a str,
    slug: &'a str,
    title: &'a str,
    changes: &'a [Change],
    prompt: &'a str,
}

pub fn run(args: UpdateArgs) -> Result<()> {
    let repo = config::load()?;
    let collected = changes::collect(&repo, &SystemGit { root: &repo.root })?;
    if collected.pending.is_empty() {
        ui::print_success("Every change is covered by an update post. Nothing to do.");
        return Ok(());
    }
    let pending = collected.pending;
    // `--select` means scripted: no follow-up questions, flags and defaults decide.
    let interactive = prompt::is_interactive() && !args.json && args.select.is_empty();

    let selected = if !args.select.is_empty() {
        select_by_ids(&pending, &args.select)?
    } else if interactive {
        pick(&pending)?
    } else {
        return Err(anyhow!(
            "No changes selected (no interactive terminal to prompt)"
        ))
        .hint("Pass --select <id,...> or --select all. `eimer product status` lists ids.");
    };
    if selected.is_empty() {
        bail!("Nothing selected");
    }
    let chosen: Vec<Change> = selected.iter().map(|&i| pending[i].clone()).collect();

    let default_title = short_title(chosen[0].summary());
    let title = match &args.title {
        Some(title) => title.trim().to_string(),
        None if interactive => prompt::text_or("Title", &default_title, |v| {
            v.trim().is_empty().then(|| "Title is required".to_string())
        })?,
        None => default_title,
    };
    let media = match args.media {
        Some(media) => media,
        None if interactive => {
            let options = ["auto", "screenshot", "demo (Stageshow)", "none"].map(String::from);
            [Media::Auto, Media::Screenshot, Media::Demo, Media::None]
                [prompt::select("Media", &options, 0)?]
        }
        None => Media::Auto,
    };
    let store = args.store
        || (interactive && prompt::confirm("Refresh App Store screenshots too?", false)?);

    let leftover_internal: Vec<&Change> = pending
        .iter()
        .enumerate()
        .filter(|(i, c)| c.internal.is_some() && !selected.contains(i))
        .map(|(_, c)| c)
        .collect();
    let ignore = !leftover_internal.is_empty()
        && (args.ignore_internal
            || (interactive
                && prompt::confirm(
                    &format!(
                        "Hide the {} unchecked internal change{} for good?",
                        leftover_internal.len(),
                        if leftover_internal.len() == 1 {
                            ""
                        } else {
                            "s"
                        }
                    ),
                    true,
                )?));

    let date = match &args.date {
        Some(date) if is_iso_date(date) => date.clone(),
        Some(date) => bail!("--date must be YYYY-MM-DD, got '{date}'"),
        None => chrono::Local::now().format("%Y-%m-%d").to_string(),
    };
    let slug = format!("{date}-{}", posts::slugify(&title));
    let draft_path = repo
        .updates_dir()
        .join(format!("{slug}.{}.md", repo.primary_language()));
    if draft_path.exists() {
        return Err(anyhow!("{} already exists", repo.relative(&draft_path)))
            .hint("Pick another --title, or finish the existing draft.");
    }
    let platforms = platforms_of(&repo, &chosen);
    let doc = draft_document(&title, &date, &platforms, &chosen, media, store);
    std::fs::create_dir_all(repo.updates_dir()).context("Failed to create the updates folder")?;
    std::fs::write(&draft_path, doc.render())
        .with_context(|| format!("Failed to write {}", draft_path.display()))?;
    let draft = repo.relative(&draft_path);

    if ignore {
        posts::add_ignored(
            &repo.ignore_file(),
            leftover_internal
                .iter()
                .map(|c| IgnoredChange {
                    id: c.id.clone(),
                    text: c.summary().to_string(),
                })
                .collect(),
        )?;
    }

    let prompt_text = template::render(
        &template::load(&repo)?,
        &PromptInput {
            repo: &repo,
            draft: &draft,
            slug: &slug,
            title: &title,
            changes: &chosen,
            platforms: &platforms,
            media: media.as_str(),
            store,
        },
    );

    if args.json {
        return ui::print_json(&UpdateOutput {
            draft: &draft,
            slug: &slug,
            title: &title,
            changes: &chosen,
            prompt: &prompt_text,
        });
    }
    ui::print_success(&format!("Draft written to {draft}"));
    if ignore {
        ui::print_info(
            &format!("Hid {} internal changes", leftover_internal.len()),
            Some(&repo.relative(&repo.ignore_file())),
        );
    }
    deliver_prompt(&prompt_text, &args)
}

fn deliver_prompt(prompt_text: &str, args: &UpdateArgs) -> Result<()> {
    if let Some(out) = &args.out {
        std::fs::write(out, prompt_text).with_context(|| format!("Failed to write {out}"))?;
        ui::print_success(&format!("Prompt written to {out}"));
        return Ok(());
    }
    if args.print {
        println!("\n{prompt_text}");
        return Ok(());
    }
    match ui::with_spinner("Copying prompt to clipboard", || {
        copy_to_clipboard(prompt_text)
    }) {
        Ok(Copied::Tool) => {
            ui::print_success("Prompt copied. Paste it into your coding agent.");
            Ok(())
        }
        Ok(Copied::Terminal) => {
            ui::print_success(
                "Prompt sent to your terminal's clipboard (OSC 52). Paste it into your coding agent.",
            );
            ui::print_info(
                "No clipboard tool on this machine, so the terminal carries it over.",
                Some(
                    "If nothing pastes, your terminal blocks OSC 52; rerun with --print or --out <file>.",
                ),
            );
            Ok(())
        }
        Err(err) if err.downcast_ref::<NoClipboard>().is_some() => {
            ui::print_warning(&format!("{err}; printing the prompt instead."), None);
            println!("\n{prompt_text}");
            Ok(())
        }
        Err(err) => Err(err),
    }
}

/// Resolves ids (or unique prefixes); `all` selects every user-facing change.
fn select_by_ids(pending: &[Change], ids: &[String]) -> Result<Vec<usize>> {
    if ids.iter().any(|id| id == "all") {
        return Ok((0..pending.len())
            .filter(|&i| pending[i].internal.is_none())
            .collect());
    }
    let mut selected = BTreeSet::new();
    for id in ids.iter().map(|id| id.trim()).filter(|id| !id.is_empty()) {
        let matches: Vec<usize> = (0..pending.len())
            .filter(|&i| pending[i].id.starts_with(id) || id.starts_with(&pending[i].id))
            .collect();
        match matches.as_slice() {
            [i] => {
                selected.insert(*i);
            }
            [] => {
                return Err(anyhow!("No pending change '{id}'"))
                    .hint("`eimer product status --all` lists pending ids.");
            }
            _ => bail!("'{id}' matches several changes; use more characters"),
        }
    }
    Ok(selected.into_iter().collect())
}

fn pick(pending: &[Change]) -> Result<Vec<usize>> {
    let width = terminal_size::terminal_size()
        .map(|(terminal_size::Width(w), _)| w as usize)
        .unwrap_or(100);
    let items: Vec<String> = pending.iter().map(|c| label(c, width)).collect();
    let checked: Vec<bool> = pending.iter().map(|c| c.internal.is_none()).collect();
    prompt::multi_select(
        "Changes for this post (space toggles, enter confirms)",
        &items,
        &checked,
    )
}

fn label(change: &Change, width: usize) -> String {
    let kind = change.kind.as_deref().unwrap_or("?");
    let mut meta = Vec::new();
    if !change.platforms.is_empty() {
        meta.push(change.platforms.join(","));
    }
    meta.push(match change.releases.as_slice() {
        [first, rest @ ..] if !rest.is_empty() => format!("{first} +{}", rest.len()),
        releases => releases.join(", "),
    });
    if let Some(reason) = &change.internal {
        meta.push(reason.clone());
    }
    let meta = format!("  {}", meta.join(" · "));
    let date = change.date.as_deref().unwrap_or("          ");
    let prefix = format!("{date} {kind:<8} ");
    let budget = width
        .saturating_sub(prefix.chars().count() + meta.chars().count() + 8)
        .max(24);
    let mut summary = change.summary().to_string();
    if summary.chars().count() > budget {
        summary = summary.chars().take(budget - 1).collect::<String>() + "…";
    }
    let text = format!("{prefix}{summary}{}", ui::dim(&meta));
    if change.internal.is_some() {
        ui::dim(&format!("{prefix}{summary}{meta}"))
    } else {
        text
    }
}

/// First sentence of a changelog line, cut at a word boundary: a starting point the
/// agent rewrites. Keeps the file name short.
fn short_title(summary: &str) -> String {
    let sentence = summary
        .split_once(". ")
        .map_or(summary, |(first, _)| first)
        .trim()
        .trim_end_matches('.');
    if sentence.chars().count() <= 48 {
        return sentence.to_string();
    }
    let mut title = String::new();
    for word in sentence.split_whitespace() {
        if title.chars().count() + word.chars().count() + 1 > 48 {
            break;
        }
        if !title.is_empty() {
            title.push(' ');
        }
        title.push_str(word);
    }
    title
}

/// Platform tags of the changes, in config order.
fn platforms_of(repo: &Repo, chosen: &[Change]) -> Vec<String> {
    let used: BTreeSet<&String> = chosen.iter().flat_map(|c| &c.platforms).collect();
    repo.config
        .platforms
        .keys()
        .filter(|p| used.contains(p))
        .cloned()
        .collect()
}

fn draft_document(
    title: &str,
    date: &str,
    platforms: &[String],
    chosen: &[Change],
    media: Media,
    store: bool,
) -> Document {
    let mut releases: Vec<String> = Vec::new();
    for release in chosen.iter().flat_map(|c| &c.releases) {
        if !releases.contains(release) {
            releases.push(release.clone());
        }
    }
    let mut doc = Document::default();
    doc.set("title", Value::Str(title.to_string()));
    doc.set("date", Value::Str(date.to_string()));
    doc.set("platforms", Value::List(platforms.to_vec()));
    doc.set("releases", Value::List(releases));
    doc.set(
        "changes",
        Value::List(chosen.iter().map(|c| c.id.clone()).collect()),
    );
    doc.set("media", Value::Str(media.as_str().to_string()));
    doc.set("store", Value::Str(store.to_string()));
    doc.set("status", Value::Str("draft".into()));
    let changes = chosen
        .iter()
        .map(|c| format!("- {}", template::indent_continuation(&c.text)))
        .collect::<Vec<_>>()
        .join("\n");
    doc.body = format!(
        "<!-- Draft from `eimer product update`. Rewrite for users, then set status: published. -->\n\n{changes}\n"
    );
    doc
}

#[cfg(test)]
mod tests {
    use super::*;

    fn change(id: &str, internal: bool) -> Change {
        Change {
            id: id.into(),
            hash: Some(id.into()),
            releases: vec!["app@1.0.0".into()],
            products: vec!["app".into()],
            text: format!("Change {id}."),
            kind: Some("feat".into()),
            scope: None,
            date: Some("2026-11-12".into()),
            platforms: vec!["mac".into()],
            internal: internal.then(|| "ci commit".to_string()),
        }
    }

    #[test]
    fn selects_by_prefix_and_all() {
        let pending = vec![change("abc1234", false), change("abd9999", true)];
        assert_eq!(select_by_ids(&pending, &["abc".into()]).unwrap(), [0]);
        assert_eq!(select_by_ids(&pending, &["all".into()]).unwrap(), [0]);
        assert!(select_by_ids(&pending, &["ab".into()]).is_err());
        assert!(select_by_ids(&pending, &["zzz".into()]).is_err());
    }

    #[test]
    fn short_titles() {
        assert_eq!(short_title("Mac planner."), "Mac planner");
        assert_eq!(
            short_title(
                "Fix the Mac website build's missing Sparkle runtime search path, which broke launch. More."
            ),
            "Fix the Mac website build's missing Sparkle"
        );
    }

    #[test]
    fn draft_lists_changes() {
        let doc = draft_document(
            "Mac app",
            "2026-11-12",
            &["mac".to_string()],
            &[change("abc1234", false)],
            Media::Auto,
            false,
        );
        let rendered = doc.render();
        assert!(rendered.contains("changes: [abc1234]"));
        assert!(rendered.contains("status: draft"));
        assert!(rendered.contains("- Change abc1234."));
        assert_eq!(template::indent_continuation("a\n\nb"), "a\n\n  b");
        let parsed = super::super::frontmatter::parse(&rendered).unwrap();
        assert_eq!(parsed.list("releases").unwrap(), ["app@1.0.0"]);
    }
}
