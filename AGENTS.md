# Eimer Scripts

Personal developer CLI toolkit - automates Azure DevOps and GitHub workflows (PRs, pipelines, releases, tasks) from the terminal.

## Docs
- [Product](docs/product.md)
- [Tech Stack](docs/tech-stack.md)
- [Architecture](docs/architecture.md)
- [Feature Specs](docs/features/)

## Layout
- `crates/eimer` - the `eimer` CLI (Rust). All user-facing commands live here.
- `packages/release` - TypeScript `@tapio/release`, still published to Azure Artifacts for `bunx` in company pipelines. Keep behavior in sync with `crates/eimer/src/commands/release`.
- `packages/ui` - TS output helpers used by `packages/release`.
- `packages/helpers` - standalone ad-hoc scripts (tsx), not part of the CLI.

## Rust CLI

```bash
cargo build                 # debug build -> target/debug/eimer
cargo run -- task list      # run a command
cargo test                  # unit tests
cargo clippy --all-targets -- -D warnings
cargo fmt
```

Releases: bump `version` in `crates/eimer/Cargo.toml`, merge, then push tag `eimer/v<version>`. CI builds all platforms and publishes the GitHub Release.

## TypeScript (`@tapio/release`)

```bash
cd packages/release && bun run typecheck && bun test
```

Changes under `packages/release` require a changeset (`bunx changeset`).

## Coding Conventions (Rust)
- **Edition 2024**, stable toolchain
- **CLI**: clap derive; az-style nested subcommands for modes (`task list mine`), flags for filters (`--all`, `--top`, `--json`)
- **Command structure**: one folder per command group under `src/commands/<group>/`, one file per command plus shared helpers; each group exports `<Group>Args` and `run`
- **Shared code**: `src/core/` (config, context, azure, process, ui, prompt, http, error). Prefer extending core over per-command duplicates.
- **Subprocesses**: always via `core::process` (no shell); `az` scoping via `core::azure::AzScope`
- **Errors**: `anyhow` with `.context("Failed to <verb> ...")`; attach user hints with `core::error::HintExt::hint`
- **Output**: human output via `core::ui`; `--json` prints raw JSON with stable field names; no spinners with `--json`
- **Tests**: `#[cfg(test)]` unit tests next to the code; smoke-test commands against fake `az`/`gh`/`git` scripts on `PATH`
- **File size**: keep files under ~500 LOC; split as needed
- **Commits**: Conventional Commits (`feat|fix|refactor|build|ci|chore|docs|style|perf|test`)

## Coding Conventions (TypeScript, packages/release)
- Bun only (`bun install`, `bun run`, `bun test`); never npm
- Bunli for commands, Zod v4 for schemas, ESM only, strict TS
- Bunli 0.9 needs `argumentKind: "flag"` on boolean options for bare `--flag` usage
