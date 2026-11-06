# eimer

Developer CLI for Azure DevOps and GitHub workflows: tasks, pull requests, pipelines and release changelogs from the terminal.

## Install

Linux / macOS:

```sh
curl -fsSL https://raw.githubusercontent.com/eimerreis/eimer-cli/main/install.sh | sh
```

Windows (PowerShell):

```powershell
irm https://raw.githubusercontent.com/eimerreis/eimer-cli/main/install.ps1 | iex
```

The installer downloads the binary for your platform from GitHub Releases, verifies its SHA-256 checksum, installs it to `~/.local/bin` (Windows: `%LOCALAPPDATA%\eimer\bin`) and creates `task`, `pr`, `pipeline` and `release` shortcuts. Options: `EIMER_VERSION=0.2.0`, `EIMER_INSTALL_DIR=...`, `EIMER_NO_LINKS=1`.

Update with:

```sh
eimer upgrade          # latest release
eimer upgrade --check  # only check
```

Requirements: [Azure CLI](https://learn.microsoft.com/cli/azure/install-azure-cli) with `az extension add --name azure-devops`; [GitHub CLI](https://cli.github.com/) for GitHub pull requests.

## Commands

`task ...` is short for `eimer task ...` (same for `pr`, `pipeline`, `release`).

| Command | Description |
|---------|-------------|
| `task list` / `task list mine` | Not-started tasks assigned to you in the current sprint (`--all` for every state) |
| `task list team` | Active tasks in the team's current sprint |
| `task list children <id>` | Child tasks of a work item |
| `task create <title>` / `start <id>` / `close <id>` / `show <id>` / `recent` | Task workflows |
| `task context show\|list\|use <name>` | Which Azure DevOps org/project task commands target |
| `pr create\|list\|show\|comments\|open\|copy` | Pull requests (Azure DevOps and GitHub) |
| `pipeline runs\|list\|show\|open\|trigger\|watch` | Azure DevOps pipelines |
| `release changelog\|approve\|configure` | Release changelogs (Teams posting) and approvals |
| `eimer configure` | Shared defaults in `~/.config/eimer/config.json` |
| `eimer upgrade` / `eimer link` | Self-update / (re)create the shortcuts |

Run any command with `--help` for flags. Most commands support `--json`.

## Contexts

Task commands can target a specific Azure DevOps project regardless of the current directory. Define contexts in `~/.config/eimer/config.json`:

```json
{
  "defaultContext": "tapio",
  "contexts": {
    "tapio": { "org": "tapioone", "project": "tapio", "team": "My Team", "areaPath": "tapio\\My Area" },
    "homag": { "org": "homag-group", "project": "onehomag" }
  }
}
```

Then pick one per directory with `task context use homag` (writes `.eimerrc`) or globally with `--global`. Resolution order: `--context` flag, `EIMER_CONTEXT`, nearest `.eimerrc`, `defaultContext`, otherwise the git remote.

## Development

See [AGENTS.md](AGENTS.md). The company package `@tapio/release` (`packages/release`, TypeScript) is still published separately for `bunx` usage in pipelines.
