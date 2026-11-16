//! `eimer secrets` against a real git repo with fake `age-keygen`, `op` and `sops` on PATH.
#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const OPERATOR_KEY: &str = "age1operator0000000000000000000000000000000000000000000000000";
const HOST_KEY: &str = "age1host00000000000000000000000000000000000000000000000000000";

struct Fixture {
    _dir: tempfile::TempDir,
    repo: PathBuf,
    bin: PathBuf,
    /// Arguments of every `sops` call, one line each.
    sops_log: PathBuf,
}

fn script(bin: &Path, name: &str, body: &str) {
    let path = bin.join(name);
    std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("repo");
    let bin = dir.path().join("bin");
    std::fs::create_dir_all(&repo).unwrap();
    std::fs::create_dir_all(&bin).unwrap();
    // git reports the resolved path (macOS: /var -> /private/var)
    let repo = repo.canonicalize().unwrap();
    assert!(
        Command::new("git")
            .args(["init", "-q"])
            .current_dir(&repo)
            .status()
            .unwrap()
            .success()
    );
    let sops_log = dir.path().join("sops.log");
    script(&bin, "age-keygen", &format!("echo {OPERATOR_KEY}"));
    script(
        &bin,
        "op",
        &format!(
            r#"[ "$3" = eimerdev ] || {{ echo "item not found" >&2; exit 1; }}
echo '{{"fields":[{{"label":"age public key","value":"{HOST_KEY}"}}]}}'"#
        ),
    );
    script(
        &bin,
        "sops",
        &format!(
            r#"echo "$@" >> '{}'
case "$3" in decrypt) echo "DB_PASS=hunter2";; esac"#,
            sops_log.display()
        ),
    );
    Fixture {
        _dir: dir,
        repo,
        bin,
        sops_log,
    }
}

fn eimer(f: &Fixture, args: &[&str]) -> Output {
    let path = format!("{}:{}", f.bin.display(), std::env::var("PATH").unwrap());
    let key_file = f.repo.join("keys.txt");
    std::fs::write(&key_file, "AGE-SECRET-KEY-FAKE\n").unwrap();
    Command::new(env!("CARGO_BIN_EXE_eimer"))
        .current_dir(&f.repo)
        .env("PATH", path)
        .env("SOPS_AGE_KEY_FILE", key_file)
        .env("NO_COLOR", "1")
        .args(args)
        .output()
        .unwrap()
}

fn ok(f: &Fixture, args: &[&str]) -> String {
    let out = eimer(f, args);
    assert!(
        out.status.success(),
        "eimer {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn read(f: &Fixture, path: &str) -> String {
    std::fs::read_to_string(f.repo.join(path)).unwrap()
}

fn sops_calls(f: &Fixture) -> Vec<String> {
    std::fs::read_to_string(&f.sops_log)
        .unwrap_or_default()
        .lines()
        .map(|l| l.replace(&f.repo.display().to_string(), "<repo>"))
        .collect()
}

#[test]
fn init_makes_the_operator_recipient_of_every_environment() {
    let f = fixture();
    ok(&f, &["secrets", "init"]);

    let manifest: serde_json::Value =
        serde_json::from_str(&read(&f, "secrets/recipients.json")).unwrap();
    assert_eq!(
        manifest,
        serde_json::json!({
            "recipients": {"operator": OPERATOR_KEY},
            "environments": {"development": ["operator"], "production": ["operator"]}
        })
    );
    assert!(read(&f, ".sops.yaml").contains("path_regex: (^|/)secrets/production\\.enc\\.env$"));
    assert!(!eimer(&f, &["secrets", "init"]).status.success());
}

#[test]
fn adding_a_host_reads_its_key_from_1password_and_reencrypts_existing_files() {
    let f = fixture();
    ok(&f, &["secrets", "init"]);
    std::fs::write(
        f.repo.join("secrets/development.enc.env"),
        "DB_PASS=ENC[...]\n",
    )
    .unwrap();

    ok(
        &f,
        &[
            "secrets",
            "recipients",
            "add",
            "eimerdev",
            "--env",
            "development",
        ],
    );

    let manifest: serde_json::Value =
        serde_json::from_str(&read(&f, "secrets/recipients.json")).unwrap();
    assert_eq!(manifest["recipients"]["eimerdev"], HOST_KEY);
    assert_eq!(
        manifest["environments"]["development"],
        serde_json::json!(["operator", "eimerdev"])
    );
    assert_eq!(
        manifest["environments"]["production"],
        serde_json::json!(["operator"])
    );
    assert!(read(&f, ".sops.yaml").contains(&format!("{OPERATOR_KEY},{HOST_KEY}")));
    // production has no file yet, so only development is re-encrypted
    assert_eq!(
        sops_calls(&f),
        vec!["--config <repo>/.sops.yaml updatekeys -y <repo>/secrets/development.enc.env"]
    );
}

#[test]
fn adding_requires_an_environment_and_a_known_host() {
    let f = fixture();
    ok(&f, &["secrets", "init"]);
    assert!(
        !eimer(&f, &["secrets", "recipients", "add", "eimerdev"])
            .status
            .success()
    );
    assert!(
        !eimer(
            &f,
            &[
                "secrets",
                "recipients",
                "add",
                "unknown",
                "--env",
                "development"
            ]
        )
        .status
        .success()
    );
}

#[test]
fn removing_a_host_warns_to_rotate_what_it_could_read() {
    let f = fixture();
    ok(&f, &["secrets", "init"]);
    ok(
        &f,
        &[
            "secrets",
            "recipients",
            "add",
            "eimerdev",
            "--env",
            "development",
        ],
    );
    std::fs::write(
        f.repo.join("secrets/development.enc.env"),
        "DB_PASS=ENC[...]\n",
    )
    .unwrap();

    let out = ok(&f, &["secrets", "recipients", "remove", "eimerdev"]);

    assert!(out.contains("Rotate those secrets"), "{out}");
    let manifest: serde_json::Value =
        serde_json::from_str(&read(&f, "secrets/recipients.json")).unwrap();
    assert_eq!(manifest["recipients"].get("eimerdev"), None);
    assert!(!read(&f, ".sops.yaml").contains(HOST_KEY));
}

#[test]
fn decrypt_writes_only_to_ignored_files() {
    let f = fixture();
    ok(&f, &["secrets", "init"]);
    std::fs::write(
        f.repo.join("secrets/production.enc.env"),
        "DB_PASS=ENC[...]\n",
    )
    .unwrap();

    let refused = eimer(&f, &["secrets", "decrypt", "production", "-o", ".env"]);
    assert!(!refused.status.success());
    assert!(!f.repo.join(".env").exists());

    std::fs::write(f.repo.join(".gitignore"), ".env\n").unwrap();
    ok(&f, &["secrets", "decrypt", "production", "-o", ".env"]);
    assert_eq!(read(&f, ".env"), "DB_PASS=hunter2\n");
    let mode = std::fs::metadata(f.repo.join(".env"))
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o600);
}

#[test]
fn get_needs_an_existing_file() {
    let f = fixture();
    ok(&f, &["secrets", "init"]);
    assert!(
        !eimer(&f, &["secrets", "get", "production", "DB_PASS"])
            .status
            .success()
    );
    std::fs::write(
        f.repo.join("secrets/production.enc.env"),
        "DB_PASS=ENC[...]\n",
    )
    .unwrap();
    ok(&f, &["secrets", "get", "production", "DB_PASS"]);
    assert_eq!(
        sops_calls(&f),
        vec![
            r#"--config <repo>/.sops.yaml decrypt --extract ["DB_PASS"] <repo>/secrets/production.enc.env"#
        ]
    );
}
