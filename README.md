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
| `eimer time login\|projects\|init\|suggest\|book\|none\|status` | Book PR work to codeunity time ([below](#time-tracking)) |
| `eimer configure` | Shared defaults in `~/.config/eimer/config.json` |
| `eimer upgrade` / `eimer link` | Self-update / (re)create the shortcuts |

Run any command with `--help` for flags. Most commands support `--json`.

## Time tracking

`eimer time` books PR work to codeunity time and records it in the PR body, so a required CI check can block merging until every PR says how much time it cost, or that none is billed:

```
Time: 1.5h → time#12345        # booked
Time: none (internal tooling)  # explicitly not billable
```

```bash
eimer time login                 # opens the browser, receives the redirect on localhost:1995
eimer time projects              # find the project id
eimer time init <projectId>      # writes .time.json in the repo (commit it)
eimer time suggest               # duration suggestion from the PR's commits
eimer time book 1.5h             # create or update the record, write the Time line
eimer time none "internal tooling"
eimer time status                # exit 1 when the PR has no valid Time line
```

- Login uses the Keycloak client `time-cli-moritz` and receives the redirect on `http://localhost:1995/callback`. The token is kept in the macOS keychain (`eimer.time`; elsewhere `~/.config/eimer/time-token.json`, mode 0600) and refreshes automatically. Override with `EIMER_TIME_CLIENT_ID`, `EIMER_TIME_REDIRECT_URI`, `EIMER_TIME_API_URL`. A non-loopback redirect URI (e.g. the Raycast extension's client) falls back to pasting the redirect URL.
- With one project in `.time.json` no alias is needed; with several pass `--project <alias>`.
- Rerunning `book` updates the existing record instead of booking twice; invoiced records are left alone.
- The CI check, PR template and agent instructions for client repos live in [eimerreis/freelance-ops](https://github.com/eimerreis/freelance-ops).

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

## Release changelogs

`release changelog` finds the last production release by looking for a successful prod stage. If a pipeline's prod stage isn't detected automatically, pass it once:

```sh
release changelog --pipeline "tapio.B2C.UI" --prod-stage "deploy-tapiousers"
```

When that stage finds a release, it is remembered for the pipeline (`release.prodStages` in `~/.config/tapio-release/config.json`), so next time `release changelog --pipeline "tapio.B2C.UI"` is enough. Order: `--prod-stage-name` > remembered per pipeline > `release.prodStageName` > auto-detection. `release configure --show` lists remembered stages.

## Development

See [AGENTS.md](AGENTS.md). Every pull request needs a changeset (`bunx changeset`, or `bunx changeset --empty` when nothing ships); releases are cut automatically from `main`. The company package `@tapio/release` (`packages/release`, TypeScript) is still published separately for `bunx` usage in pipelines.
