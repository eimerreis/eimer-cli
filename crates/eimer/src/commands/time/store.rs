//! Token storage: macOS keychain, otherwise a 0600 file in the eimer config dir.

use anyhow::{Context, Result};

use crate::core::process::{run_text, run_with_stdin, try_run_text};

const SERVICE: &str = "eimer.time";
const ACCOUNT: &str = "default";

pub fn write(value: &str) -> Result<()> {
    write_as(ACCOUNT, value)
}

pub fn read() -> Result<Option<String>> {
    read_as(ACCOUNT)
}

pub fn delete() -> Result<()> {
    delete_as(ACCOUNT)
}

fn write_as(account: &str, value: &str) -> Result<()> {
    if cfg!(target_os = "macos") {
        // `security -i` reads the command from stdin, so the token never appears in argv.
        let encoded = super::auth::base64_url(value.as_bytes());
        run_with_stdin(
            ["security", "-i"],
            &format!("add-generic-password -U -a {account} -s {SERVICE} -w {encoded}\n"),
        )
        .context("Failed to write the token to the keychain")?;
        if read_as(account)?.as_deref() != Some(value) {
            anyhow::bail!("Failed to write the token to the keychain");
        }
        return Ok(());
    }
    let _ = account;
    let path = file_path()?;
    std::fs::create_dir_all(path.parent().expect("config dir has a parent"))?;
    std::fs::write(&path, value).context("Failed to write the token file")?;
    restrict_permissions(&path)
}

fn read_as(account: &str) -> Result<Option<String>> {
    if cfg!(target_os = "macos") {
        let Some(raw) = try_run_text([
            "security",
            "find-generic-password",
            "-a",
            account,
            "-s",
            SERVICE,
            "-w",
        ]) else {
            return Ok(None);
        };
        let bytes = super::auth::base64_url_decode(raw.trim())
            .context("Failed to decode the stored token")?;
        return Ok(Some(String::from_utf8(bytes)?));
    }
    let _ = account;
    match std::fs::read_to_string(file_path()?) {
        Ok(raw) => Ok(Some(raw)),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err).context("Failed to read the token file"),
    }
}

fn delete_as(account: &str) -> Result<()> {
    if cfg!(target_os = "macos") {
        let _ = run_text([
            "security",
            "delete-generic-password",
            "-a",
            account,
            "-s",
            SERVICE,
        ]);
        return Ok(());
    }
    let _ = account;
    match std::fs::remove_file(file_path()?) {
        Err(err) if err.kind() != std::io::ErrorKind::NotFound => {
            Err(err).context("Failed to delete the token file")
        }
        _ => Ok(()),
    }
}

fn file_path() -> Result<std::path::PathBuf> {
    Ok(crate::core::config::config_dir()?.join("time-token.json"))
}

#[cfg(unix)]
fn restrict_permissions(path: &std::path::Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
        .context("Failed to restrict the token file permissions")
}

#[cfg(not(unix))]
fn restrict_permissions(_path: &std::path::Path) -> Result<()> {
    Ok(())
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    /// Touches the real login keychain: `cargo test -- --ignored keychain`.
    #[test]
    #[ignore]
    fn keychain_round_trip() {
        let account = "eimer-selftest";
        let value = r#"{"accessToken":"a b \"c\" $HOME","n":1}"#;
        write_as(account, value).unwrap();
        write_as(account, &format!("{value}2")).unwrap();
        assert_eq!(read_as(account).unwrap(), Some(format!("{value}2")));
        delete_as(account).unwrap();
        assert_eq!(read_as(account).unwrap(), None);
    }
}
