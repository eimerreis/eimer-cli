//! External tools eimer installs itself (see AGENTS.md, "External tools").
//!
//! Each [`Tool`] pins a version and the SHA-256 of its official release archive per platform.
//! [`ensure`] returns a usable binary: the pinned install under `<data>/tools/<name>/<version>/`,
//! else one on `PATH` that meets the minimum version, else it downloads, verifies and unpacks
//! the pinned release.

use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use sha2::{Digest, Sha256};

use super::error::HintExt;
use super::{config, http, process, ui};

pub struct Tool {
    pub name: &'static str,
    pub version: &'static str,
    /// Oldest version on `PATH` that eimer accepts instead of installing its own.
    pub min_version: &'static str,
    /// Download URL for an asset name; `{version}` is replaced.
    pub url: &'static str,
    /// `(rust target, asset name, sha256)` per supported platform.
    pub assets: &'static [(&'static str, &'static str, &'static str)],
}

pub const TYPST: Tool = Tool {
    name: "typst",
    version: "0.15.1",
    min_version: "0.15.0",
    url: "https://github.com/typst/typst/releases/download/v{version}/{asset}",
    assets: &[
        (
            "aarch64-apple-darwin",
            "typst-aarch64-apple-darwin.tar.xz",
            "48f62ed034aa3a7978309579ac6ca00045e2ef0da73114e8af27cfd8e74dc05a",
        ),
        (
            "x86_64-apple-darwin",
            "typst-x86_64-apple-darwin.tar.xz",
            "7f9fdd9584866245de9a79e0add8f9236fae6f40a8a45e2c4771ccc14db4e0fa",
        ),
        (
            "aarch64-unknown-linux-musl",
            "typst-aarch64-unknown-linux-musl.tar.xz",
            "5aa8d74a3d906e60ea12a66ac2f37f8eef1b14cbad7182a745e393a10c23dcee",
        ),
        (
            "x86_64-unknown-linux-musl",
            "typst-x86_64-unknown-linux-musl.tar.xz",
            "a6d077d0a95eed5a2eba715b2dae06be954f624ccbf85758a03f389ded33118c",
        ),
        (
            "aarch64-pc-windows-msvc",
            "typst-aarch64-pc-windows-msvc.zip",
            "4ab28e1b71ec3184d38d580ab797f499b6770d952b6b19167be5cea5c2662e14",
        ),
        (
            "x86_64-pc-windows-msvc",
            "typst-x86_64-pc-windows-msvc.zip",
            "19ce3551153c2fe7ee9fa2f95208310c8f4d3209fedb699e0333faf8913f6736",
        ),
    ],
};

/// Path to a usable binary of `tool`, installing the pinned version if needed.
/// `quiet` suppresses the progress spinner (e.g. for `--json`).
pub fn ensure(tool: &Tool, quiet: bool) -> Result<PathBuf> {
    let managed = managed_binary(tool)?;
    if managed.exists() {
        return Ok(managed);
    }
    if let Some(found) = on_path(tool) {
        return Ok(found);
    }
    let label = format!("Installing {} {}", tool.name, tool.version);
    ui::spin_unless(quiet, &label, || install(tool, &managed))
        .with_context(|| format!("Failed to install {} {}", tool.name, tool.version))
        .hint(format!(
            "Or install {} {}+ yourself and put it on PATH",
            tool.name, tool.min_version
        ))?;
    if !quiet {
        ui::print_success(&format!(
            "Installed {} {} to {}",
            tool.name,
            tool.version,
            config::with_home_path(&managed)
        ));
    }
    Ok(managed)
}

fn exe_name(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    }
}

fn managed_binary(tool: &Tool) -> Result<PathBuf> {
    Ok(config::data_dir()?
        .join("tools")
        .join(tool.name)
        .join(tool.version)
        .join(exe_name(tool.name)))
}

/// `tool` on PATH when `<tool> --version` reports at least `min_version`.
fn on_path(tool: &Tool) -> Option<PathBuf> {
    let output = process::try_run_text([tool.name, "--version"])?;
    let min = semver::Version::parse(tool.min_version).ok()?;
    (parse_version(&output)? >= min).then(|| PathBuf::from(tool.name))
}

/// First `x.y.z` in a `--version` output such as `typst 0.15.1 (9dfd3a08)`.
fn parse_version(output: &str) -> Option<semver::Version> {
    output
        .split_whitespace()
        .find_map(|word| semver::Version::parse(word.trim_start_matches('v')).ok())
}

fn current_target() -> Result<String> {
    let arch = std::env::consts::ARCH;
    Ok(match std::env::consts::OS {
        "linux" => format!("{arch}-unknown-linux-musl"),
        "macos" => format!("{arch}-apple-darwin"),
        "windows" => format!("{arch}-pc-windows-msvc"),
        other => bail!("Unsupported operating system '{other}'"),
    })
}

fn asset_for<'a>(tool: &'a Tool, target: &str) -> Result<(&'a str, &'a str)> {
    tool.assets
        .iter()
        .find(|(t, _, _)| *t == target)
        .map(|(_, asset, sha)| (*asset, *sha))
        .ok_or_else(|| anyhow!("No {} release for {target}", tool.name))
}

fn install(tool: &Tool, target_path: &Path) -> Result<()> {
    let (asset, sha256) = asset_for(tool, &current_target()?)?;
    let url = tool
        .url
        .replace("{version}", tool.version)
        .replace("{asset}", asset);
    let archive = download(&url)?;
    verify(&archive, sha256)?;

    let dir = target_path
        .parent()
        .ok_or_else(|| anyhow!("Invalid tool path {}", target_path.display()))?;
    std::fs::create_dir_all(dir).with_context(|| format!("Failed to create {}", dir.display()))?;
    // Unpack next to the target and rename, so an interrupted install never leaves a half binary.
    let staged = tempfile::NamedTempFile::new_in(dir)?;
    let binary = extract(asset, &archive, &exe_name(tool.name))?;
    std::fs::write(staged.path(), binary)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(staged.path(), std::fs::Permissions::from_mode(0o755))?;
    }
    staged
        .persist(target_path)
        .with_context(|| format!("Failed to write {}", target_path.display()))?;
    Ok(())
}

fn download(url: &str) -> Result<Vec<u8>> {
    let client = http::builder().timeout(None).build()?;
    let mut response = client
        .get(url)
        .send()
        .with_context(|| format!("Failed to download {url}"))?;
    if !response.status().is_success() {
        bail!("GET {url} returned {}", response.status());
    }
    let mut bytes = Vec::new();
    response.read_to_end(&mut bytes)?;
    Ok(bytes)
}

fn verify(bytes: &[u8], expected: &str) -> Result<()> {
    let actual = hex::encode(Sha256::digest(bytes));
    if actual != expected {
        bail!("Checksum mismatch (expected {expected}, got {actual}); refusing to install");
    }
    Ok(())
}

/// The file named `binary` inside a `.tar.xz` or `.zip` release archive.
fn extract(asset: &str, archive: &[u8], binary: &str) -> Result<Vec<u8>> {
    if asset.ends_with(".tar.xz") {
        extract_tar_xz(archive, binary)
    } else if asset.ends_with(".zip") {
        extract_zip(archive, binary)
    } else {
        bail!("Unsupported archive {asset}")
    }
}

fn extract_tar_xz(archive: &[u8], binary: &str) -> Result<Vec<u8>> {
    let mut tar = Vec::new();
    lzma_rs::xz_decompress(&mut std::io::Cursor::new(archive), &mut tar)
        .map_err(|e| anyhow!("Failed to decompress the archive: {e}"))?;
    let mut entries = tar::Archive::new(tar.as_slice());
    for entry in entries.entries()? {
        let mut entry = entry?;
        if entry.path()?.file_name().is_some_and(|n| n == binary) {
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes)?;
            return Ok(bytes);
        }
    }
    bail!("The archive has no {binary}")
}

/// Windows ships `tar.exe` (bsdtar), which reads zip archives.
fn extract_zip(archive: &[u8], binary: &str) -> Result<Vec<u8>> {
    let dir = tempfile::tempdir()?;
    let zip = dir.path().join("archive.zip");
    std::fs::write(&zip, archive)?;
    process::run_text([
        "tar",
        "-xf",
        &zip.to_string_lossy(),
        "-C",
        &dir.path().to_string_lossy(),
    ])?;
    find_file(dir.path(), binary)
        .and_then(|path| std::fs::read(path).ok())
        .ok_or_else(|| anyhow!("The archive has no {binary}"))
}

fn find_file(dir: &Path, name: &str) -> Option<PathBuf> {
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if let Some(found) = find_file(&path, name) {
                return Some(found);
            }
        } else if path.file_name().is_some_and(|n| n == name) {
            return Some(path);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tar_xz(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut builder = tar::Builder::new(Vec::new());
        for (path, bytes) in files {
            let mut header = tar::Header::new_gnu();
            header.set_size(bytes.len() as u64);
            header.set_mode(0o755);
            header.set_cksum();
            builder.append_data(&mut header, path, *bytes).unwrap();
        }
        let tar = builder.into_inner().unwrap();
        let mut xz = Vec::new();
        lzma_rs::xz_compress(&mut tar.as_slice(), &mut xz).unwrap();
        xz
    }

    #[test]
    fn extracts_the_binary_from_a_tar_xz() {
        let archive = tar_xz(&[
            ("typst-x/README.md", b"readme"),
            ("typst-x/typst", b"binary"),
        ]);
        assert_eq!(
            extract("typst-x.tar.xz", &archive, "typst").unwrap(),
            b"binary"
        );
        assert!(extract("typst-x.tar.xz", &archive, "sops").is_err());
    }

    #[test]
    fn rejects_a_wrong_checksum() {
        let sha = hex::encode(Sha256::digest(b"data"));
        assert!(verify(b"data", &sha).is_ok());
        assert!(verify(b"tampered", &sha).is_err());
    }

    #[test]
    fn reads_versions_from_version_output() {
        assert_eq!(
            parse_version("typst 0.15.1 (9dfd3a08)"),
            Some(semver::Version::new(0, 15, 1))
        );
        assert_eq!(
            parse_version("sops v3.9.0"),
            Some(semver::Version::new(3, 9, 0))
        );
        assert_eq!(parse_version("typst dev"), None);
    }

    #[test]
    fn typst_has_a_release_for_every_eimer_target() {
        for target in [
            "aarch64-apple-darwin",
            "x86_64-apple-darwin",
            "x86_64-unknown-linux-musl",
            "aarch64-unknown-linux-musl",
            "x86_64-pc-windows-msvc",
        ] {
            assert!(asset_for(&TYPST, target).is_ok(), "{target}");
        }
    }
}
