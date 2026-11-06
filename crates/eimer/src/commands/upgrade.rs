//! `eimer upgrade` (self-update from GitHub Releases) and `eimer link` (multi-call symlinks).

use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use clap::Args;
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::MULTICALL_GROUPS;
use crate::core::ui;

pub const REPO: &str = "eimerreis/eimer-cli";
/// Rust CLI releases are tagged `eimer/v<semver>`; `release/v*` tags belong to @tapio/release.
pub const TAG_PREFIX: &str = "eimer/v";

#[derive(Args)]
pub struct UpgradeArgs {
    /// Only check whether a newer version is available
    #[arg(long)]
    check: bool,
    /// Install a specific version (e.g. 0.2.0) instead of the latest
    #[arg(long, value_name = "VERSION")]
    to: Option<String>,
    /// Reinstall even if already up to date
    #[arg(long)]
    force: bool,
}

#[derive(Args)]
pub struct LinkArgs {
    /// Directory for the links (defaults to the directory of the eimer binary)
    #[arg(long, value_name = "DIR")]
    dir: Option<PathBuf>,
    /// Remove the links instead of creating them
    #[arg(long)]
    remove: bool,
}

#[derive(Deserialize, Debug)]
struct Release {
    tag_name: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<Asset>,
}

#[derive(Deserialize, Debug)]
struct Asset {
    name: String,
    browser_download_url: String,
}

/// Release asset name for the running platform, e.g. `eimer-x86_64-unknown-linux-musl`.
pub fn asset_name() -> Result<String> {
    let arch = match std::env::consts::ARCH {
        "x86_64" => "x86_64",
        "aarch64" => "aarch64",
        other => bail!("Unsupported architecture '{other}'"),
    };
    let target = match std::env::consts::OS {
        "linux" => format!("{arch}-unknown-linux-musl"),
        "macos" => format!("{arch}-apple-darwin"),
        "windows" => format!("{arch}-pc-windows-msvc"),
        other => bail!("Unsupported operating system '{other}'"),
    };
    let ext = if cfg!(windows) { ".exe" } else { "" };
    Ok(format!("eimer-{target}{ext}"))
}

fn client() -> Result<reqwest::blocking::Client> {
    Ok(reqwest::blocking::Client::builder()
        .user_agent(concat!("eimer/", env!("CARGO_PKG_VERSION")))
        .build()?)
}

fn get(client: &reqwest::blocking::Client, url: &str) -> Result<reqwest::blocking::Response> {
    let mut request = client
        .get(url)
        .header("Accept", "application/vnd.github+json");
    if let Some(token) = std::env::var("GITHUB_TOKEN")
        .ok()
        .or_else(|| std::env::var("GH_TOKEN").ok())
    {
        request = request.bearer_auth(token);
    }
    let response = request
        .send()
        .with_context(|| format!("Request to {url} failed"))?;
    if !response.status().is_success() {
        bail!("GET {url} returned {}", response.status());
    }
    Ok(response)
}

fn parse_tag(tag: &str) -> Option<semver::Version> {
    semver::Version::parse(tag.strip_prefix(TAG_PREFIX)?).ok()
}

/// Newest stable `eimer/v*` release, or the one matching `version`.
fn find_release(
    client: &reqwest::blocking::Client,
    version: Option<&semver::Version>,
) -> Result<(semver::Version, Release)> {
    let releases: Vec<Release> = get(
        client,
        &format!("https://api.github.com/repos/{REPO}/releases?per_page=50"),
    )?
    .json()?;
    select_release(releases, version).ok_or_else(|| match version {
        Some(v) => anyhow!("Release {TAG_PREFIX}{v} not found"),
        None => anyhow!("No eimer releases found in {REPO}"),
    })
}

fn select_release(
    releases: Vec<Release>,
    version: Option<&semver::Version>,
) -> Option<(semver::Version, Release)> {
    releases
        .into_iter()
        .filter(|r| !r.draft)
        .filter_map(|r| parse_tag(&r.tag_name).map(|v| (v, r)))
        .filter(|(v, r)| match version {
            Some(wanted) => v == wanted,
            None => !r.prerelease && v.pre.is_empty(),
        })
        .max_by(|(a, _), (b, _)| a.cmp(b))
}

pub fn run(args: UpgradeArgs) -> Result<()> {
    upgrade(args).context("Failed to upgrade eimer")
}

fn upgrade(args: UpgradeArgs) -> Result<()> {
    let current = semver::Version::parse(env!("CARGO_PKG_VERSION"))?;
    let wanted = args
        .to
        .as_deref()
        .map(|v| semver::Version::parse(v.trim_start_matches('v')))
        .transpose()?;
    let client = client()?;
    let (version, release) = ui::with_spinner("Checking for updates", || {
        find_release(&client, wanted.as_ref())
    })?;

    if args.check {
        if version > current {
            ui::print_info(
                &format!("eimer {version} is available (installed: {current})."),
                Some("Run `eimer upgrade`."),
            );
        } else {
            ui::print_success(&format!("eimer {current} is up to date."));
        }
        return Ok(());
    }

    if version == current && !args.force {
        ui::print_success(&format!("eimer {current} is already up to date."));
        return Ok(());
    }
    if version < current && wanted.is_none() && !args.force {
        ui::print_info(
            &format!("Installed eimer {current} is newer than the latest release {version}."),
            None,
        );
        return Ok(());
    }

    let name = asset_name()?;
    let find = |n: &str| release.assets.iter().find(|a| a.name == n);
    let binary =
        find(&name).ok_or_else(|| anyhow!("Release {} has no asset '{name}'", release.tag_name))?;
    let checksum = find(&format!("{name}.sha256"))
        .ok_or_else(|| anyhow!("Release {} has no checksum for '{name}'", release.tag_name))?;

    let bytes = ui::with_spinner(
        &format!("Downloading eimer {version}"),
        || -> Result<Vec<u8>> {
            let mut buf = Vec::new();
            get(&client, &binary.browser_download_url)?.read_to_end(&mut buf)?;
            Ok(buf)
        },
    )?;
    let expected = get(&client, &checksum.browser_download_url)?.text()?;
    verify_checksum(&bytes, &expected)?;

    let exe = std::env::current_exe()?;
    let dir = exe
        .parent()
        .ok_or_else(|| anyhow!("Cannot resolve install directory"))?;
    let mut tmp =
        tempfile::NamedTempFile::new_in(dir).or_else(|_| tempfile::NamedTempFile::new())?;
    std::io::Write::write_all(&mut tmp, &bytes)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(tmp.path(), std::fs::Permissions::from_mode(0o755))?;
    }
    self_replace::self_replace(tmp.path())
        .context("Failed to replace the running binary (try with sudo?)")?;

    // Windows .cmd shims keep pointing at eimer.exe, unix symlinks too: nothing to relink.
    ui::print_success(&format!("Upgraded eimer {current} -> {version}."));
    Ok(())
}

/// Accepts `sha256sum` output (`<hex>  <file>`) or a bare hex digest.
fn verify_checksum(bytes: &[u8], expected: &str) -> Result<()> {
    let expected = expected
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_lowercase();
    let actual = hex::encode(Sha256::digest(bytes));
    if expected != actual {
        bail!("Checksum mismatch (expected {expected}, got {actual}); refusing to install");
    }
    Ok(())
}

pub fn link(args: LinkArgs) -> Result<()> {
    (|| -> Result<()> {
        let exe = std::env::current_exe()?.canonicalize()?;
        let dir = match args.dir {
            Some(dir) => dir,
            None => exe
                .parent()
                .ok_or_else(|| anyhow!("Cannot resolve binary directory"))?
                .to_path_buf(),
        };
        for group in MULTICALL_GROUPS {
            let path = link_path(&dir, group);
            if args.remove {
                if is_our_link(&path, &exe) {
                    std::fs::remove_file(&path)?;
                    ui::print_success(&format!("Removed {}", path.display()));
                }
                continue;
            }
            if path.exists() || path.symlink_metadata().is_ok() {
                if is_our_link(&path, &exe) {
                    continue;
                }
                ui::print_warning(
                    &format!(
                        "Skipping {}: a different file already exists.",
                        path.display()
                    ),
                    None,
                );
                continue;
            }
            create_link(&exe, &path, group)?;
            ui::print_success(&format!("Linked {} -> eimer {group}", path.display()));
        }
        Ok(())
    })()
    .context("Failed to create links")
}

fn link_path(dir: &Path, group: &str) -> PathBuf {
    if cfg!(windows) {
        dir.join(format!("{group}.cmd"))
    } else {
        dir.join(group)
    }
}

fn is_our_link(path: &Path, exe: &Path) -> bool {
    if cfg!(windows) {
        std::fs::read_to_string(path).is_ok_and(|s| s.contains("eimer"))
    } else {
        std::fs::read_link(path)
            .ok()
            .and_then(|t| path.parent().map(|p| p.join(t)))
            .and_then(|t| t.canonicalize().ok())
            == Some(exe.to_path_buf())
    }
}

#[cfg(unix)]
fn create_link(exe: &Path, path: &Path, _group: &str) -> Result<()> {
    std::os::unix::fs::symlink(exe, path)
        .with_context(|| format!("Failed to create {}", path.display()))
}

#[cfg(windows)]
fn create_link(exe: &Path, path: &Path, group: &str) -> Result<()> {
    let shim = format!("@\"{}\" {group} %*\r\n", exe.display());
    std::fs::write(path, shim).with_context(|| format!("Failed to create {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(tag: &str, prerelease: bool) -> Release {
        Release {
            tag_name: tag.into(),
            draft: false,
            prerelease,
            assets: vec![],
        }
    }

    #[test]
    fn picks_newest_stable_eimer_release() {
        let releases = vec![
            release("release/v9.0.0", false),
            release("eimer/v0.2.0", false),
            release("eimer/v0.10.0-beta.1", true),
            release("eimer/v0.3.1", false),
        ];
        let (version, r) = select_release(releases, None).unwrap();
        assert_eq!(version.to_string(), "0.3.1");
        assert_eq!(r.tag_name, "eimer/v0.3.1");
    }

    #[test]
    fn picks_requested_version_even_if_prerelease() {
        let releases = vec![
            release("eimer/v0.2.0", false),
            release("eimer/v0.3.0-rc.1", true),
        ];
        let wanted = semver::Version::parse("0.3.0-rc.1").unwrap();
        assert_eq!(select_release(releases, Some(&wanted)).unwrap().0, wanted);
    }

    #[test]
    fn checksum_verification() {
        let digest = hex::encode(Sha256::digest(b"hello"));
        assert!(verify_checksum(b"hello", &format!("{digest}  eimer-x")).is_ok());
        assert!(verify_checksum(b"hello", "deadbeef").is_err());
    }

    #[test]
    fn asset_name_matches_platform() {
        let name = asset_name().unwrap();
        assert!(name.starts_with("eimer-"));
    }
}
