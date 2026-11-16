//! Document templates, brand fonts and examples, compiled into the binary.
//!
//! Typst reads templates and fonts from disk, so [`unpack`] writes them once per content hash
//! to the cache directory and returns that directory.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use sha2::{Digest, Sha256};

use crate::core::config;

macro_rules! embed {
    ($($path:literal),* $(,)?) => {
        &[$(($path, include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/documents/", $path)))),*]
    };
}

/// `(relative path, contents)` of every embedded file.
pub const FILES: &[(&str, &[u8])] = embed![
    "lib/doc.typ",
    "templates/contract.typ",
    "templates/invoice.typ",
    "templates/letter.typ",
    "templates/quote.typ",
    "templates/reminder.typ",
    "templates/time-report.typ",
    "brand/logo.svg",
    "brand/logo-name.svg",
    "brand/logo-name-stacked.svg",
    "brand/mark-hitze.svg",
    "brand/fonts/Figtree-Bold.ttf",
    "brand/fonts/Figtree-BoldItalic.ttf",
    "brand/fonts/Figtree-Italic.ttf",
    "brand/fonts/Figtree-Medium.ttf",
    "brand/fonts/Figtree-Regular.ttf",
    "brand/fonts/Figtree-SemiBold.ttf",
    "brand/fonts/OFL-Figtree.txt",
    "brand/fonts/OFL-Unbounded.txt",
    "brand/fonts/Unbounded-Bold.ttf",
    "brand/fonts/Unbounded-ExtraBold.ttf",
    "brand/fonts/Unbounded-Regular.ttf",
    "examples/contract.yaml",
    "examples/invoice.yaml",
    "examples/letter.yaml",
    "examples/quote.yaml",
    "examples/reminder.yaml",
    "examples/sender.yaml",
    "examples/time-report.yaml",
];

/// Contents of an embedded file.
pub fn file(path: &str) -> Result<&'static [u8]> {
    FILES
        .iter()
        .find(|(p, _)| *p == path)
        .map(|(_, bytes)| *bytes)
        .ok_or_else(|| anyhow!("No embedded file {path}"))
}

/// Writes the embedded files to `<cache>/eimer/documents/<hash>` unless already there.
pub fn unpack() -> Result<PathBuf> {
    let dir = config::cache_dir()?.join("documents").join(content_hash());
    unpack_to(&dir)?;
    Ok(dir)
}

fn unpack_to(dir: &Path) -> Result<()> {
    let marker = dir.join(".complete");
    if marker.exists() {
        return Ok(());
    }
    for (path, bytes) in FILES {
        let target = dir.join(path);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create {}", parent.display()))?;
        }
        std::fs::write(&target, bytes)
            .with_context(|| format!("Failed to write {}", target.display()))?;
    }
    std::fs::write(&marker, b"").with_context(|| format!("Failed to write {}", marker.display()))
}

/// Short hash of all embedded files, so a new release never reuses stale templates.
fn content_hash() -> String {
    let mut hasher = Sha256::new();
    for (path, bytes) in FILES {
        hasher.update(path.as_bytes());
        hasher.update(bytes);
    }
    hex::encode(&hasher.finalize()[..6])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_template_has_an_example() {
        for (path, _) in FILES.iter().filter(|(p, _)| p.starts_with("templates/")) {
            let name = path
                .trim_start_matches("templates/")
                .replace(".typ", ".yaml");
            assert!(
                file(&format!("examples/{name}")).is_ok(),
                "no example for {path}"
            );
        }
    }

    #[test]
    fn unpacks_all_files_once() {
        let dir = tempfile::tempdir().unwrap();
        unpack_to(dir.path()).unwrap();
        assert!(dir.path().join("templates/invoice.typ").exists());
        assert!(dir.path().join("brand/fonts/Figtree-Regular.ttf").exists());

        std::fs::remove_file(dir.path().join("lib/doc.typ")).unwrap();
        unpack_to(dir.path()).unwrap();
        assert!(
            !dir.path().join("lib/doc.typ").exists(),
            "marker skips unpacking"
        );
    }
}
