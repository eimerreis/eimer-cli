//! Where age public keys come from: the operator's local key file and the 1Password
//! "Server" item a host's setup writes (field `age public key`).

use std::path::PathBuf;

use anyhow::{Context, Result, anyhow};
use serde::Deserialize;

use crate::core::error::HintExt;
use crate::core::process::{run_json, run_text};

use super::manifest::validate_key;

/// Label of the 1Password field holding a host's age public key.
pub const ONEPASSWORD_FIELD: &str = "age public key";

/// The key file SOPS reads: `$SOPS_AGE_KEY_FILE`, else its platform default.
pub fn operator_key_file() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("SOPS_AGE_KEY_FILE") {
        return Some(PathBuf::from(path));
    }
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            let home = std::env::var_os("HOME")?;
            let home = PathBuf::from(home);
            Some(if cfg!(target_os = "macos") {
                home.join("Library/Application Support")
            } else {
                home.join(".config")
            })
        })?;
    Some(config.join("sops/age/keys.txt"))
}

/// Public key of the operator's local age key (the first one in the key file).
pub fn operator_public_key() -> Result<String> {
    let path =
        operator_key_file().ok_or_else(|| anyhow!("Cannot locate the age key file (no HOME)"))?;
    if !path.exists() {
        return Err(anyhow!("No age key at {}", path.display())).hint(format!(
            "Create one with `age-keygen -o \"{}\"` and back it up in 1Password, or pass --key age1...",
            path.display()
        ));
    }
    let out = run_text(["age-keygen", "-y", &path.to_string_lossy()])
        .context("Failed to read the public key of the operator's age key")
        .hint("Install age (brew install age / apt install age).")?;
    let key = out
        .lines()
        .map(str::trim)
        .find(|l| l.starts_with("age1"))
        .ok_or_else(|| anyhow!("{} holds no age key", path.display()))?
        .to_string();
    validate_key(&key)?;
    Ok(key)
}

#[derive(Deserialize)]
struct Item {
    #[serde(default)]
    fields: Vec<Field>,
}

#[derive(Deserialize)]
struct Field {
    #[serde(default)]
    label: String,
    #[serde(default)]
    value: Option<String>,
}

/// Age public key stored on the 1Password item titled `host`.
pub fn onepassword_public_key(host: &str) -> Result<String> {
    let item: Item = run_json(["op", "item", "get", host, "--format", "json"])
        .with_context(|| format!("Failed to read 1Password item '{host}'"))
        .hint("Sign in with `op signin`, or pass the key with --key age1...")?;
    key_from_fields(&item.fields)
        .ok_or_else(|| anyhow!("1Password item '{host}' has no '{ONEPASSWORD_FIELD}' field"))
        .hint("install-srv.sh writes it when it sets up the host; or pass --key age1...")
}

fn key_from_fields(fields: &[Field]) -> Option<String> {
    fields
        .iter()
        .filter(|f| f.label.eq_ignore_ascii_case(ONEPASSWORD_FIELD))
        .filter_map(|f| f.value.as_deref().map(str::trim))
        .find(|v| validate_key(v).is_ok())
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_key_field_by_label() {
        let item: Item = serde_json::from_value(serde_json::json!({
            "fields": [
                {"label": "username", "value": "eimer"},
                {"label": "Age Public Key", "value": " age1abcdefghijklmnopqrstuvwxyz0123456789 "},
                {"label": "notesPlain"}
            ]
        }))
        .unwrap();
        assert_eq!(
            key_from_fields(&item.fields).as_deref(),
            Some("age1abcdefghijklmnopqrstuvwxyz0123456789")
        );
    }

    #[test]
    fn ignores_a_field_without_a_valid_key() {
        let item: Item = serde_json::from_value(serde_json::json!({
            "fields": [{"label": "age public key", "value": "todo"}]
        }))
        .unwrap();
        assert_eq!(key_from_fields(&item.fields), None);
    }
}
