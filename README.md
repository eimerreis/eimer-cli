# eimer

Personal developer CLI for Azure DevOps and GitHub workflows (PRs, pipelines, releases, tasks) from the terminal.

## Quick start (forgot how? start here)

```bash
git clone https://github.com/eimerreis/eimer-cli.git ~/dev/eimer-cli
cd ~/dev/eimer-cli
./install.sh          # installs Bun if missing, then runs `bun run setup`
```

`bun run setup` does everything in one go:

1. `bun install`
2. builds `bin/eimer` (use `--all` to also build `pr`, `pipeline`, `release`, `task`)
3. symlinks the binary into `~/.local/bin` (override with `--bin-dir <dir>` or `EIMER_BIN_DIR`) and tells you the `PATH` line to add if needed
4. runs `eimer doctor`

Then:

```bash
eimer doctor                 # what's missing? (az, extension, login, gh, config, PATH)
eimer configure              # task defaults (team, area path)
eimer release configure      # release defaults (pipeline, branch, Teams channels)
eimer --help
```

## Prerequisites

`eimer doctor` checks all of these and prints the fix for anything missing.

| Tool | Needed for | Install |
|------|------------|---------|
| [Bun](https://bun.sh) >= 1.3.11 | building/updating | `curl -fsSL https://bun.sh/install \| bash` |
| git | everything | `xcode-select --install` |
| Azure CLI + `azure-devops` extension | PRs, pipelines, releases, tasks on Azure DevOps | `brew install azure-cli && az extension add --name azure-devops && az login` |
| GitHub CLI | PRs on GitHub repos | `brew install gh && gh auth login` |

Optional: set Azure DevOps defaults so commands work outside a repo:
`az devops configure --defaults organization=https://dev.azure.com/<org> project=<project>`

## Staying up to date

```bash
eimer update             # git pull --ff-only, bun install, rebuild
eimer update --no-pull   # just reinstall + rebuild the current checkout
eimer update --all       # also rebuild the standalone binaries
```

`eimer update` finds the checkout via the symlinked binary. If you moved things around, set `EIMER_HOME=/path/to/eimer-cli`.

## Shell completion

```bash
# zsh (~/.zshrc)
source <(eimer completions zsh)
# bash (~/.bashrc)
source <(eimer completions bash)
# fish
eimer completions fish > ~/.config/fish/completions/eimer.fish
```

## Commands

| Command | What it does |
|---------|--------------|
| `eimer pr create\|list\|show\|open\|comments\|copy` | Pull requests (Azure DevOps + GitHub, auto-detected from `origin`) — see [packages/pr](packages/pr/README.md) |
| `eimer pipeline runs\|list\|show\|open\|trigger\|watch` | Azure DevOps pipeline runs — see [packages/pipeline](packages/pipeline/README.md) |
| `eimer release changelog\|approve\|configure` | Release changelogs, approvals, Teams posting — see [packages/release](packages/release/README.md) |
| `eimer task create\|list\|recent\|start\|close\|show` | Azure DevOps tasks in the current sprint — see [packages/task](packages/task/README.md) |
| `eimer configure` | Task defaults; `--show` prints eimer **and** release config |
| `eimer doctor` | Check tools, logins, config and PATH |
| `eimer update` | Pull, reinstall, rebuild |
| `eimer completions <shell>` | Print completion script |

Every command supports `--help`; most support `--json`.

## Configuration

| File | Written by | Holds |
|------|------------|-------|
| `~/.config/eimer/config.json` | `eimer configure` | `task.defaultTeam`, `task.defaultAreaPath` |
| `~/.config/tapio-release/config.json` | `eimer release configure` | default pipeline, release branch, prod stage, Teams channels, areas |

`eimer configure --show` prints both.

## Development

```bash
bun run dev -- pr list       # run eimer from source
bun run typecheck            # all packages
bun run test                 # all packages
bun run build                # bin/eimer
bun run build:all            # bin/eimer + bin/pr, pipeline, release, task
bun run generate             # refresh .bunli/commands.gen.ts files
```

See [AGENTS.md](AGENTS.md) for conventions and [docs/](docs/) for product, architecture and feature specs.

The release CLI is also published as `@tapio/release` to Azure Artifacts via Changesets. Add a changeset (`bunx changeset`) to any PR that touches `packages/release`.

## Troubleshooting

- **`eimer: command not found`**: `~/.local/bin` isn't on your `PATH`. Rerun `bun run setup`; it prints the line to add.
- **`'az' was not found on your PATH`**: install the Azure CLI (see prerequisites) and run `eimer doctor`.
- **Azure DevOps returns a sign-in page**: `az login` again, or unset a stale `AZURE_DEVOPS_PAT`.
- **Build fails after a dependency bump**: Bunli and `@opentui/*` are pinned on purpose (two `@opentui/core` copies crash at startup). Bump them together and keep the root `overrides` in sync.
