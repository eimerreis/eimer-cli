//! `eimer secrets`: runtime secrets as SOPS/age-encrypted dotenv files in the repo
//! (`secrets/<environment>.enc.env`), see eimerreis/devops ADR-0019.
//!
//! `secrets/recipients.json` names who may decrypt which environment; `.sops.yaml` is
//! generated from it. Encryption and decryption are delegated to `sops`.

mod keys;
mod manifest;

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use clap::{Args, Subcommand};
use serde::Serialize;

use crate::core::error::{HintExt, SilentExit};
use crate::core::process::{run_inherit, run_text, try_git_in};
use crate::core::ui;

use manifest::{Manifest, OPERATOR, SOPS_CONFIG_FILE, environment_file};

/// `sops edit` exit code when the file was saved unchanged.
const SOPS_UNCHANGED: i32 = 200;

#[derive(Args)]
pub struct SecretsArgs {
    #[command(subcommand)]
    command: SecretsCommand,
}

#[derive(Subcommand)]
enum SecretsCommand {
    /// Create secrets/recipients.json and .sops.yaml with the operator as recipient
    Init(InitArgs),
    /// Manage who may decrypt which environment
    Recipients(RecipientsArgs),
    /// Edit an environment's secrets in $EDITOR (creates the file on first use)
    Edit(EnvArg),
    /// Print one secret
    Get(GetArgs),
    /// Decrypt an environment to stdout or a git-ignored dotenv file
    Decrypt(DecryptArgs),
}

#[derive(Args)]
struct InitArgs {
    /// Environments to create (repeatable)
    #[arg(long = "env", default_values = ["production", "development"])]
    environments: Vec<String>,
    /// Operator age public key (default: derived from the local SOPS age key file)
    #[arg(long)]
    key: Option<String>,
}

#[derive(Args)]
struct RecipientsArgs {
    #[command(subcommand)]
    command: RecipientsCommand,
}

#[derive(Subcommand)]
enum RecipientsCommand {
    /// Show recipients per environment
    List {
        /// Print machine-readable JSON
        #[arg(long, short = 'j')]
        json: bool,
    },
    /// Allow a host to decrypt environments; re-encrypts existing files
    Add {
        /// Host name; also the 1Password item title holding its age public key
        name: String,
        /// Environments the host may decrypt (repeatable, at least one)
        #[arg(long = "env", required = true)]
        environments: Vec<String>,
        /// Age public key (default: field "age public key" of the 1Password item <name>)
        #[arg(long)]
        key: Option<String>,
    },
    /// Revoke a host's access; re-encrypts existing files
    Remove {
        name: String,
        /// Environments to revoke (repeatable; default: all)
        #[arg(long = "env")]
        environments: Vec<String>,
    },
}

#[derive(Args)]
struct EnvArg {
    /// Environment, e.g. production or development
    env: String,
}

#[derive(Args)]
struct GetArgs {
    env: String,
    /// Variable name, e.g. DATABASE_PASSWORD
    key: String,
}

#[derive(Args)]
struct DecryptArgs {
    env: String,
    /// Write to this file (mode 0600) instead of stdout; inside the repo it must be git-ignored
    #[arg(long, short = 'o')]
    output: Option<PathBuf>,
}

pub fn run(args: SecretsArgs) -> Result<()> {
    let root = repo_root()?;
    match args.command {
        SecretsCommand::Init(a) => init(&root, a),
        SecretsCommand::Recipients(a) => match a.command {
            RecipientsCommand::List { json } => list(&root, json),
            RecipientsCommand::Add {
                name,
                environments,
                key,
            } => add(&root, &name, &environments, key),
            RecipientsCommand::Remove { name, environments } => remove(&root, &name, &environments),
        },
        SecretsCommand::Edit(a) => edit(&root, &a.env),
        SecretsCommand::Get(a) => get(&root, &a.env, &a.key),
        SecretsCommand::Decrypt(a) => decrypt(&root, &a.env, a.output),
    }
}

fn repo_root() -> Result<PathBuf> {
    let root = run_text(["git", "rev-parse", "--show-toplevel"])
        .context("Failed to find the git repository")?;
    Ok(PathBuf::from(root.trim()))
}

fn init(root: &Path, args: InitArgs) -> Result<()> {
    if manifest::read(root)?.is_some() {
        return Err(anyhow!("{} already exists", manifest::MANIFEST_FILE))
            .hint("Use `eimer secrets recipients add` to grant hosts access.");
    }
    let key = match args.key {
        Some(key) => key,
        None => keys::operator_public_key()?,
    };
    let manifest = Manifest::new(&key, &args.environments)?;
    manifest::save(root, &manifest)?;
    ui::print_success(&format!(
        "Created {} and {SOPS_CONFIG_FILE} for {}",
        manifest::MANIFEST_FILE,
        args.environments.join(", ")
    ));
    ui::print_info(
        "Add secrets with `eimer secrets edit <env>`, grant hosts with `eimer secrets recipients add`.",
        None,
    );
    Ok(())
}

#[derive(Serialize)]
struct RecipientRow<'a> {
    environment: &'a str,
    name: &'a str,
    key: &'a str,
    file_exists: bool,
}

fn list(root: &Path, json: bool) -> Result<()> {
    let manifest = manifest::load(root)?;
    let mut rows = Vec::new();
    for (env, members) in &manifest.environments {
        let file_exists = root.join(environment_file(env)).exists();
        for name in members {
            let key = manifest.recipients.get(name).map_or("?", String::as_str);
            rows.push(RecipientRow {
                environment: env,
                name,
                key,
                file_exists,
            });
        }
    }
    if json {
        return ui::print_json(&rows);
    }
    let table = ui::render_table(
        ["Environment", "Recipient", "Key", "File"],
        rows.iter().map(|r| {
            vec![
                r.environment.to_string(),
                r.name.to_string(),
                short_key(r.key),
                if r.file_exists { "yes" } else { "-" }.to_string(),
            ]
        }),
    );
    println!("{table}");
    Ok(())
}

fn short_key(key: &str) -> String {
    if key.len() > 16 {
        format!("{}…{}", &key[..10], &key[key.len() - 6..])
    } else {
        key.to_string()
    }
}

fn add(root: &Path, name: &str, environments: &[String], key: Option<String>) -> Result<()> {
    let mut manifest = manifest::load(root)?;
    let key = match key {
        Some(key) => key,
        None => manifest
            .recipients
            .get(name)
            .cloned()
            .map_or_else(|| keys::onepassword_public_key(name), Ok)?,
    };
    let joined = manifest.add(name, &key, environments)?;
    if joined.is_empty() {
        ui::print_info(
            &format!("{name} can already decrypt {}", environments.join(", ")),
            None,
        );
        return Ok(());
    }
    manifest::save(root, &manifest)?;
    update_keys(root, &joined)?;
    ui::print_success(&format!("{name} can now decrypt {}", joined.join(", ")));
    Ok(())
}

fn remove(root: &Path, name: &str, environments: &[String]) -> Result<()> {
    let mut manifest = manifest::load(root)?;
    let left = manifest.remove(name, environments)?;
    if left.is_empty() {
        ui::print_info(&format!("{name} had no access to remove"), None);
        return Ok(());
    }
    manifest::save(root, &manifest)?;
    update_keys(root, &left)?;
    ui::print_success(&format!("{name} can no longer decrypt {}", left.join(", ")));
    let readable: Vec<&String> = left
        .iter()
        .filter(|env| root.join(environment_file(env)).exists())
        .collect();
    if !readable.is_empty() {
        ui::print_warning(
            &format!(
                "{name} may still hold the values it decrypted from {}",
                readable
                    .iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Some(
                "Rotate those secrets: change them at the source, then `eimer secrets edit <env>`.",
            ),
        );
    }
    Ok(())
}

/// Re-encrypts the existing files of `environments` for their current recipients.
fn update_keys(root: &Path, environments: &[String]) -> Result<()> {
    for env in environments {
        let file = root.join(environment_file(env));
        if !file.exists() {
            continue;
        }
        sops(root, ["updatekeys", "-y", &file.to_string_lossy()])
            .with_context(|| format!("Failed to re-encrypt {}", environment_file(env)))
            .hint(format!(
                "Re-encrypting needs a key that can decrypt it ({OPERATOR}'s local age key)."
            ))?;
    }
    Ok(())
}

fn environment_path(root: &Path, env: &str) -> Result<PathBuf> {
    manifest::load(root)?.require_environment(env)?;
    Ok(root.join(environment_file(env)))
}

fn existing_environment_path(root: &Path, env: &str) -> Result<PathBuf> {
    let path = environment_path(root, env)?;
    if !path.exists() {
        return Err(anyhow!("{} does not exist yet", environment_file(env)))
            .hint(format!("Create it with `eimer secrets edit {env}`."));
    }
    Ok(path)
}

fn edit(root: &Path, env: &str) -> Result<()> {
    let path = environment_path(root, env)?;
    let config = root.join(SOPS_CONFIG_FILE);
    let code = run_inherit([
        "sops",
        "--config",
        &config.to_string_lossy(),
        "edit",
        &path.to_string_lossy(),
    ])
    .hint(SOPS_HINT)?;
    match code {
        0 => {
            ui::print_success(&format!("Saved {}", environment_file(env)));
            Ok(())
        }
        SOPS_UNCHANGED => {
            ui::print_info("No changes", None);
            Ok(())
        }
        code => Err(SilentExit(code).into()),
    }
}

fn get(root: &Path, env: &str, key: &str) -> Result<()> {
    let path = existing_environment_path(root, env)?;
    let extract = format!("[\"{}\"]", key.replace('\\', "\\\\").replace('"', "\\\""));
    let value = sops(
        root,
        ["decrypt", "--extract", &extract, &path.to_string_lossy()],
    )
    .with_context(|| format!("Failed to read {key} from {}", environment_file(env)))
    .hint(recipient_hint(env))?;
    println!("{}", value.trim_end_matches('\n'));
    Ok(())
}

fn decrypt(root: &Path, env: &str, output: Option<PathBuf>) -> Result<()> {
    let path = existing_environment_path(root, env)?;
    let file = path.to_string_lossy();
    let Some(output) = output else {
        let plain = sops(root, ["decrypt", &file]).hint(recipient_hint(env))?;
        print!("{plain}");
        return Ok(());
    };
    let output = std::path::absolute(&output)
        .with_context(|| format!("Failed to resolve {}", output.display()))?;
    ensure_ignored(root, &output)?;
    let plain = sops(root, ["decrypt", &file]).hint(recipient_hint(env))?;
    write_private(&output, &plain)?;
    ui::print_success(&format!("Decrypted {env} to {}", output.display()));
    Ok(())
}

/// Refuses to write plaintext into the repo unless git ignores the path.
fn ensure_ignored(root: &Path, output: &Path) -> Result<()> {
    let canonical_root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let parent = output
        .parent()
        .and_then(|p| p.canonicalize().ok())
        .unwrap_or_else(|| output.to_path_buf());
    if !parent.starts_with(&canonical_root) {
        return Ok(());
    }
    let ignored = try_git_in(root, ["check-ignore", "-q", &output.to_string_lossy()]).is_some();
    if !ignored {
        return Err(anyhow!(
            "{} is inside the repository and not git-ignored",
            output.display()
        ))
        .hint("Add it to .gitignore first, so plaintext secrets are never committed.");
    }
    Ok(())
}

fn write_private(path: &Path, content: &str) -> Result<()> {
    use std::io::Write;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(path)
        .with_context(|| format!("Failed to write {}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    }
    file.write_all(content.as_bytes())
        .with_context(|| format!("Failed to write {}", path.display()))
}

const SOPS_HINT: &str =
    "Install sops (brew install sops, or the release binary from github.com/getsops/sops).";

/// Runs `sops --config <root>/.sops.yaml <args>` and returns stdout.
fn sops<const N: usize>(root: &Path, args: [&str; N]) -> Result<String> {
    let config = root.join(SOPS_CONFIG_FILE);
    if !config.exists() {
        bail!("No {SOPS_CONFIG_FILE} in this repo");
    }
    let mut cmd = vec![
        "sops".to_string(),
        "--config".to_string(),
        config.to_string_lossy().into_owned(),
    ];
    cmd.extend(args.iter().map(|a| a.to_string()));
    match run_text(cmd) {
        Err(err) if is_not_found(&err) => Err(err).hint(SOPS_HINT),
        result => result,
    }
}

fn is_not_found(err: &anyhow::Error) -> bool {
    err.chain().any(|e| {
        e.downcast_ref::<std::io::Error>()
            .is_some_and(|io| io.kind() == std::io::ErrorKind::NotFound)
    })
}

fn recipient_hint(env: &str) -> String {
    format!("Only recipients of {env} can decrypt it; see `eimer secrets recipients list`.")
}
