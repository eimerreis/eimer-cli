//! The small YAML subset update posts use for front matter:
//!
//! ```yaml
//! title: VolleyTeam for Mac
//! platforms: [mac, iphone]
//! media:
//!   kind: image
//!   src: /updates/mac/planner.png
//! ```
//!
//! Scalars (plain or JSON-quoted), inline lists and one level of nested keys. Anything
//! else is an error, so posts stay readable by every static-site tool.

use anyhow::{Result, anyhow, bail};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Str(String),
    List(Vec<String>),
    Map(Vec<(String, String)>),
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Document {
    pub fields: Vec<(String, Value)>,
    pub body: String,
}

impl Document {
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.fields.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }

    pub fn str(&self, key: &str) -> Option<&str> {
        match self.get(key)? {
            Value::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn list(&self, key: &str) -> Option<&[String]> {
        match self.get(key)? {
            Value::List(items) => Some(items),
            _ => None,
        }
    }

    pub fn map(&self, key: &str) -> Option<&[(String, String)]> {
        match self.get(key)? {
            Value::Map(entries) => Some(entries),
            _ => None,
        }
    }

    pub fn set(&mut self, key: &str, value: Value) {
        match self.fields.iter_mut().find(|(k, _)| k == key) {
            Some((_, existing)) => *existing = value,
            None => self.fields.push((key.to_string(), value)),
        }
    }

    pub fn render(&self) -> String {
        let mut out = String::from("---\n");
        for (key, value) in &self.fields {
            match value {
                Value::Str(s) => out.push_str(&format!("{key}: {}\n", scalar(s))),
                Value::List(items) => {
                    let items: Vec<String> = items.iter().map(|i| list_item(i)).collect();
                    out.push_str(&format!("{key}: [{}]\n", items.join(", ")));
                }
                Value::Map(entries) => {
                    out.push_str(&format!("{key}:\n"));
                    for (k, v) in entries {
                        out.push_str(&format!("  {k}: {}\n", scalar(v)));
                    }
                }
            }
        }
        out.push_str("---\n");
        if !self.body.is_empty() {
            out.push('\n');
            out.push_str(self.body.trim_end());
            out.push('\n');
        }
        out
    }
}

fn needs_quotes(value: &str, extra: &[char]) -> bool {
    value.is_empty()
        || value != value.trim()
        || value.starts_with([
            '"', '\'', '[', '{', '-', '!', '&', '*', '|', '>', '%', '@', '`',
        ])
        || value.contains(": ")
        || value.contains(" #")
        || value.ends_with(':')
        || value.contains(extra)
}

fn scalar(value: &str) -> String {
    if needs_quotes(value, &[]) {
        serde_json::to_string(value).unwrap_or_default()
    } else {
        value.to_string()
    }
}

fn list_item(value: &str) -> String {
    if needs_quotes(value, &[',', ']']) {
        serde_json::to_string(value).unwrap_or_default()
    } else {
        value.to_string()
    }
}

pub fn parse(raw: &str) -> Result<Document> {
    let raw = raw
        .strip_prefix('\u{feff}')
        .unwrap_or(raw)
        .replace("\r\n", "\n");
    let rest = raw
        .strip_prefix("---\n")
        .ok_or_else(|| anyhow!("missing front matter (file must start with ---)"))?;
    let (head, body) = match rest.find("\n---") {
        Some(end) => {
            let after = &rest[end + 4..];
            if !(after.is_empty() || after.starts_with('\n')) {
                bail!("front matter must end with a line containing only ---");
            }
            (&rest[..end], after.trim_start_matches('\n'))
        }
        None if rest.starts_with("---") => ("", rest[3..].trim_start_matches('\n')),
        None => bail!("front matter is not closed with ---"),
    };

    let mut doc = Document {
        body: body.to_string(),
        ..Document::default()
    };
    let mut open_map: Option<(String, Vec<(String, String)>)> = None;

    for (index, line) in head.lines().enumerate() {
        let line_no = index + 2;
        if line.trim().is_empty() || line.trim_start().starts_with('#') {
            continue;
        }
        if let Some(nested) = line.strip_prefix("  ") {
            let Some((_, entries)) = open_map.as_mut() else {
                bail!("line {line_no}: unexpected indentation");
            };
            let (key, value) = split_key(nested, line_no)?;
            entries.push((key, parse_scalar(value, line_no)?));
            continue;
        }
        if let Some((key, entries)) = open_map.take() {
            if entries.is_empty() {
                bail!("line {}: `{key}` has no value", line_no - 1);
            }
            doc.fields.push((key, Value::Map(entries)));
        }
        let (key, value) = split_key(line, line_no)?;
        if doc.get(&key).is_some() {
            bail!("line {line_no}: duplicate key `{key}`");
        }
        let value = value.trim();
        if value.is_empty() {
            open_map = Some((key, Vec::new()));
        } else if value.starts_with('[') {
            doc.fields
                .push((key, Value::List(parse_list(value, line_no)?)));
        } else {
            doc.fields
                .push((key, Value::Str(parse_scalar(value, line_no)?)));
        }
    }
    if let Some((key, entries)) = open_map {
        if entries.is_empty() {
            bail!("`{key}` has no value");
        }
        doc.fields.push((key, Value::Map(entries)));
    }
    Ok(doc)
}

fn split_key(line: &str, line_no: usize) -> Result<(String, &str)> {
    let (key, value) = line
        .split_once(':')
        .ok_or_else(|| anyhow!("line {line_no}: expected `key: value`"))?;
    let key = key.trim();
    if key.is_empty()
        || !key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        bail!("line {line_no}: invalid key `{key}`");
    }
    Ok((key.to_string(), value))
}

fn parse_scalar(value: &str, line_no: usize) -> Result<String> {
    let value = value.trim();
    if value.starts_with('"') {
        return serde_json::from_str(value)
            .map_err(|_| anyhow!("line {line_no}: invalid quoted string {value}"));
    }
    if let Some(inner) = value.strip_prefix('\'') {
        let inner = inner
            .strip_suffix('\'')
            .ok_or_else(|| anyhow!("line {line_no}: unterminated quoted string"))?;
        return Ok(inner.replace("''", "'"));
    }
    Ok(value.to_string())
}

fn parse_list(value: &str, line_no: usize) -> Result<Vec<String>> {
    let inner = value
        .strip_prefix('[')
        .and_then(|v| v.strip_suffix(']'))
        .ok_or_else(|| anyhow!("line {line_no}: lists must be written inline as [a, b]"))?;
    let mut items = Vec::new();
    let mut chars = inner.chars().peekable();
    loop {
        while chars.peek().is_some_and(|c| c.is_whitespace()) {
            chars.next();
        }
        let Some(&first) = chars.peek() else { break };
        let mut item = String::new();
        if first == '"' {
            let mut raw = String::from('"');
            chars.next();
            let mut escaped = false;
            for c in chars.by_ref() {
                raw.push(c);
                if escaped {
                    escaped = false;
                } else if c == '\\' {
                    escaped = true;
                } else if c == '"' {
                    break;
                }
            }
            item = serde_json::from_str(&raw)
                .map_err(|_| anyhow!("line {line_no}: invalid quoted list item {raw}"))?;
            while chars.peek().is_some_and(|c| c.is_whitespace()) {
                chars.next();
            }
            match chars.next() {
                None | Some(',') => {}
                Some(c) => bail!("line {line_no}: unexpected `{c}` after list item"),
            }
        } else {
            for c in chars.by_ref() {
                if c == ',' {
                    break;
                }
                item.push(c);
            }
            item = item.trim().to_string();
        }
        if !item.is_empty() {
            items.push(item);
        }
    }
    Ok(items)
}

#[cfg(test)]
mod tests {
    use super::*;

    const POST: &str = "---
title: VolleyTeam for Mac
date: 2026-11-12
platforms: [mac, iphone]
changes: []
media:
  kind: image
  alt: \"Planner: two blocks\"
status: published
---

Body text.
";

    #[test]
    fn parses_fields_and_body() {
        let doc = parse(POST).unwrap();
        assert_eq!(doc.str("title"), Some("VolleyTeam for Mac"));
        assert_eq!(doc.list("platforms").unwrap(), ["mac", "iphone"]);
        assert!(doc.list("changes").unwrap().is_empty());
        let media = doc.map("media").unwrap();
        assert_eq!(media[1], ("alt".into(), "Planner: two blocks".into()));
        assert_eq!(doc.body, "Body text.\n");
    }

    #[test]
    fn render_round_trips() {
        let doc = parse(POST).unwrap();
        let rendered = doc.render();
        assert_eq!(parse(&rendered).unwrap(), doc);
        assert!(rendered.contains("alt: \"Planner: two blocks\""));
    }

    #[test]
    fn quotes_list_items_with_commas() {
        let mut doc = Document::default();
        doc.set("tags", Value::List(vec!["a, b".into(), "c".into()]));
        let rendered = doc.render();
        assert_eq!(
            parse(&rendered).unwrap().list("tags").unwrap(),
            ["a, b", "c"]
        );
    }

    #[test]
    fn rejects_broken_front_matter() {
        assert!(parse("title: x").is_err());
        assert!(parse("---\ntitle: x\n").is_err());
        assert!(parse("---\n  kind: image\n---\n").is_err());
        assert!(parse("---\ntitle: a\ntitle: b\n---\n").is_err());
        assert!(parse("---\nlist: [a, b\n---\n").is_err());
    }
}
