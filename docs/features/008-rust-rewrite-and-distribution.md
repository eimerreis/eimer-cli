# Feature: Rust Rewrite & Automatic Distribution

## Overview
Replace the Bun/TypeScript CLIs (`eimer`, `pr`, `pipeline`, `task`, `config`) with a single Rust binary, and make installing and updating automatic: prebuilt binaries on GitHub Releases, a one-line installer, self-update, and one binary that also answers to `task`/`pr`/`pipeline`/`release`.

Previously, using the CLI required cloning the repo, installing Bun, `bun install`, building five ~60-90 MB binaries into a git-ignored `bin/` and adding it to `PATH`.

## User Stories
- As a developer on a new machine, I want one command to install eimer without cloning or installing Bun.
- As a developer, I want `eimer upgrade` to update in place, with the download verified.
- As a developer, I want to keep typing `task list` / `pr show` without separate binaries.

## Decisions
- **Big-bang port** of all command groups; behavior, flags, short flags, positional args and `--json` field names match the TS CLIs (intentional deviations are listed in the PR).
- **`@tapio/release` stays TypeScript** (`packages/release`) because company pipelines run it via `bunx`. Rust `eimer release` shares its config (`~/.config/tapio-release/config.json`).
- **Tags**: Rust releases use `eimer/v<semver>`; `release/v*` remains for `@tapio/release`. Install/upgrade filter by prefix, so GitHub's "Latest" badge is irrelevant (eimer releases are created with `--latest=false`).
- **Targets**: `x86_64`/`aarch64-unknown-linux-musl` (static, built with cargo-zigbuild), `x86_64`/`aarch64-apple-darwin`, `x86_64-pc-windows-msvc`.
- **TLS**: rustls with the `ring` provider (no cmake/C cross toolchain needed for musl).
- **Multi-call**: `argv[0]` file stem `task|pr|pipeline|release` is rewritten to `eimer <name>`; `eimer link` creates symlinks (Windows: `.cmd` shims).

## Implementation Plan
- [x] Cargo workspace + `crates/eimer` with shared `core` (config, context, azure, process, ui, prompt, http, error)
- [x] Port `task` (incl. contexts, feature 007) and `configure`
- [x] Port `pipeline`, `pr`, `release`
- [x] `eimer upgrade` (GitHub Releases, SHA-256 verification, self-replace) and `eimer link`
- [x] `release-eimer.yml` (build matrix + publish on tag) and `rust.yml` (fmt, clippy, tests on 3 OSes)
- [x] `install.sh` / `install.ps1`
- [x] Remove replaced TS packages; update docs

## Acceptance Criteria
- [x] `cargo test`, `cargo clippy -D warnings`, `cargo fmt --check` pass
- [x] Static musl binaries build for x86_64 and aarch64 and make HTTPS requests
- [x] Installer installs a pinned and the latest version, creates links, rejects a bad checksum (local e2e with a fake release server)
- [x] `eimer upgrade` upgrades 0.1.0 -> 0.2.0, links follow, re-run is a no-op, bad checksum refused
- [ ] First real `eimer/v0.1.0` release published by CI
- [ ] Commands verified against real Azure DevOps / GitHub
