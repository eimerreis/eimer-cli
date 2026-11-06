# Tech Stack

## Overview
The `eimer` CLI is a single Rust binary. The TypeScript `@tapio/release` package (Bun) remains for company pipelines that run it via `bunx`.

## Rust CLI (`crates/eimer`)

### Language & toolchain
- **Rust** stable, edition 2024, Cargo workspace at the repo root
- Release profile: LTO, `opt-level = "z"`, stripped, `panic = "abort"` (static Linux binaries are ~3 MB)

### Key crates
| Crate | Purpose |
|-------|---------|
| `clap` (derive) | Command/flag parsing, help, az-style nested subcommands |
| `anyhow` | Error propagation with context; hints via `core::error::HintExt` |
| `serde`, `serde_json` (`preserve_order`) | Config, `az`/REST JSON, `--json` output in source key order |
| `comfy-table` (`custom_styling`) | ASCII tables wrapping to terminal width (ANSI-aware) |
| `owo-colors`, `indicatif`, `dialoguer` | Colors, spinners, interactive prompts |
| `chrono` | Relative timestamps |
| `reqwest` (blocking, `rustls-no-provider`) + `rustls` (`ring`) | HTTPS for the release REST client, Teams webhooks, self-update |
| `self-replace`, `sha2`, `semver`, `tempfile` | Self-update with checksum verification |

`ring` is used instead of `aws-lc-rs` because it cross-compiles to static musl targets without cmake.

### External tools (runtime)
- **`az`** + `azure-devops` extension: tasks, PRs, pipelines (and release outside CI)
- **`gh`**: GitHub PRs
- **`git`**: branch/remote detection

### Testing & quality
- `cargo test` (unit tests next to the code, `#[cfg(test)]`)
- `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check`

### Build & distribution
- `cargo build --release` locally; CI builds static musl (cargo-zigbuild), macOS and Windows binaries on `eimer/v*` tags
- Installed via `install.sh` / `install.ps1`, updated via `eimer upgrade`

## TypeScript (`packages/release`, `packages/ui`, `packages/helpers`)
- **Bun** runtime and package manager (`bun install`, `bun test`), **Bunli** CLI framework, **Zod v4**
- `packages/release` is bundled with `bun build --target bun` and published as `@tapio/release`
- `packages/helpers` uses tsx + cmd-ts for ad-hoc scripts
