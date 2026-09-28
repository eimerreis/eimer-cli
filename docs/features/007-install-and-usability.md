# Feature: Fast Install & Self-Service Setup

## Overview
Make it possible to go from a fresh machine (or a long break) to a working `eimer` in one command, and to diagnose and fix setup problems without reading code.

## User Stories
- As the owner, I want `./install.sh` to install Bun if needed, build `eimer` and put it on my `PATH`, so I don't have to remember the steps.
- As the owner, I want `eimer doctor` to tell me exactly which tool, login or config is missing and how to fix it.
- As the owner, I want `eimer update` to pull and rebuild the checkout the binary came from.
- As the owner, I want tab completion and a correct `--version`.

## Technical Approach

### Install
- `install.sh`: bootstraps Bun, then `bun run setup`.
- `scripts/setup.ts`: `bun install` -> `bun run build` (or `build:all`) -> symlink into `~/.local/bin` (`--bin-dir`, `EIMER_BIN_DIR`) -> PATH hint -> completion hint -> `eimer doctor`.
- Builds use `bun build --compile` directly; `bunli build` requires a per-package `node_modules` that a hoisted install doesn't create.
- No `postinstall`: `bunli generate` failed on fresh installs because the auto-installed `bun` peer dependency shim shadowed the real `bun`. Generated files are committed; `bun run generate` refreshes them. `trustedDependencies: ["bun"]` keeps the shim working for other scripts.
- Bunli pinned to exact versions; root `overrides` force a single `@opentui/core`/`@opentui/react` version (two copies crash with `Environment variable "OTUI_DUMP_CAPTURES" is already registered`).

### New eimer commands
- `doctor [--json]`: bun (>= 1.3.11), git, az, azure-devops extension, az login, az devops defaults, gh + auth, eimer config, release config, clipboard tool, eimer on PATH (and whether it points at this checkout). Exits 1 on failures.
- `update [--no-pull] [--all]`: refuses on a dirty tree, `git pull --ff-only`, `bun install`, rebuild. Repo located via `EIMER_HOME`, the symlinked binary's real path, or the source location in dev mode.
- `completions <zsh|bash|fish>`: shell glue calling the hidden `eimer __complete <words...>` callback, which walks the registered command tree (`name:description` lines).
- `release configure` is now reachable from eimer; `eimer configure` only manages task defaults and `--show` prints both config files.

### Shared helpers (`@scripts/ui`)
- `runText`/`runJson` (single implementation, replaces copies in pr/pipeline/task/release) check `Bun.which` first and throw an install hint for `az`/`gh`/`git`.
- `openUrl`: `open` / `xdg-open` / `wslview` / `gio` / `start`, printing the URL when none is available.

### Fixes found along the way
- Boolean options now declare `argumentKind: "flag"`; before, Bunli 0.9 parsed a bare `--json`/`--show`/`--all` as `false`. A test in `packages/eimer/src/lib/flags.test.ts` guards this.
- `--version` reads from each package's `package.json` instead of a hardcoded `0.1.0`.

### CI
- `.github/workflows/ci.yml`: frozen install, typecheck, test, `build:all`, binary smoke test on Ubuntu and macOS.
