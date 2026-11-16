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
| `eimer time login\|projects\|init\|suggest\|add\|none\|status\|pending\|book\|report` | Record work to codeunity time ([below](#time-tracking)) |
| `eimer doc new\|render` | Invoices, quotes, letters, reminders, contracts and time reports as PDF ([below](#documents)) |
| `eimer secrets init\|recipients\|edit\|get\|decrypt` | Runtime secrets as SOPS-encrypted files in the repo ([below](#runtime-secrets)) |
| `eimer product init\|status\|update\|check` | Public update posts from Changesets changelogs ([below](#product-updates)) |
| `eimer configure` | Shared defaults in `~/.config/eimer/config.json` |
| `eimer upgrade` / `eimer link` | Self-update / (re)create the shortcuts |

Run any command with `--help` for flags. Most commands support `--json`.

## Time tracking

`eimer time` records work to codeunity time. Every PR adds a pending entry under `.time/` (like a changeset) or states that no time is billed; a required CI check blocks merging otherwise. `eimer time book` later books all pending entries and appends them to `TIME.md`, so the repo shows when time was last booked and what is still open.

```bash
eimer time login                  # opens the browser, receives the redirect on localhost:1995
eimer time projects               # find the project id
eimer time init <projectId>       # writes .time.json in the repo (commit it), installs the pre-push hook
eimer time hook install           # pre-push hook for other clones (shared by their worktrees)
eimer time suggest                # focus time from your prompts since the last `add`
eimer time suggest --include <dir> # also count sessions started in <dir>, e.g. a scratch folder
eimer time add 1.5h -d "what"     # writes .time/<date>-<id>.md (commit it with the PR)
eimer time preview [--json]       # what the next push would record for this branch (writes nothing)
eimer time add --auto [--commit] [--include <dir>]  # what the hook does: record focus into this branch's entry
eimer time none "internal tooling" # PR body: Time: none (internal tooling)
eimer time status                 # exit 1 when the PR has neither
eimer time pending                # open entries and the last booking
eimer time book [--dry-run]       # one record per project and day, then TIME.md
eimer time report [--from --to] [--json]  # hours per project: window (default this week), unbilled, budget
eimer time report --project <id|name> [--from --to] [-o file.pdf]  # time report PDF (Zeitbuchungen) for a client
```

`eimer time report --project` writes the project's records in the window (default: all of them) as `time-report` YAML and renders it with [`eimer doc`](#documents). The YAML stays next to the PDF, so you can fix wording and re-render with `eimer doc render <file>.yaml`. With `--json` it prints the data instead. The client name comes from the project's billing customer, falling back to the project name. Only records with the invoiced flag ("Abgr.") set in time count as billed: the report still lists the others, marks them "nicht abgerechnet" and shows their hours apart from the billed on-site and remote totals. In the YAML such an entry carries `billed: false`.

Entry file:

```markdown
---
hours: 1.5
date: 2026-11-12
project: web        # only when .time.json lists several projects
---
Ledger for pending time entries
```

- `suggest` reads local agent transcripts (Claude Code in `~/.claude/projects`, Codex in `~/.codex/sessions`; T3 Code runs both) whose working directory belongs to this repo, including its worktrees. It suggests **focus time**: from one of your prompts to the next when they are at most `--gap` apart (default 10m), plus 2 minutes before every prompt. When the agent works alone for longer, only your return counts. Prompts sent by tools (task notifications, PR updates, subagents, title generation) are ignored, and parallel sessions count once. It also shows how long the agent was active. The window starts where the last `add` ended (kept per repo in `~/.config/eimer/time-activity.json`), else at midnight; override with `--since`. Time outside the agent is not seen; the user decides the recorded duration.
- `--include <dir>` (repeatable, on `suggest` and `add --auto`) also counts sessions whose working directory is `<dir>` or below it. Use it when work started in a scratch folder and belongs to this repo after all, e.g. `eimer time suggest --include ~/.t3/scratch/<name> --since 2026-11-13`, then `eimer time add <duration>`.
- **pre-push hook:** on push it records the focus time since the last window into the branch's entry (`.time/*.md`, created on the first push with the branch's commit subjects as description), commits it as `chore(time): record <hours>` and stops the push once; push again to include it. Later pushes add to the same entry, but only stop the push when the rounded hours change. Hours you edited by hand grow by new focus only. Skip once with `EIMER_TIME_SKIP=1`; errors never block a push (the CI check still requires an entry). Repos with several projects in `.time.json` need `eimer time add --project`.
- `book` merges the descriptions of entries of the same project and day into one record (`<repo>: a; b`), deletes the booked entries and appends a section to `TIME.md`. Commit both in a PR; a PR that only touches `.time/`, `TIME.md` and `.changeset/` passes the check.
- Login uses the Keycloak client `time-cli-moritz` and receives the redirect on `http://localhost:1995/callback`. The token is kept in the macOS keychain (`eimer.time`; elsewhere `~/.config/eimer/time-token.json`, mode 0600) and refreshes automatically. Override with `EIMER_TIME_CLIENT_ID`, `EIMER_TIME_REDIRECT_URI`, `EIMER_TIME_API_URL`. A non-loopback redirect URI (e.g. the Raycast extension's client) falls back to pasting the redirect URL.
- With one project in `.time.json` no alias is needed; with several pass `--project <alias>`.
- The CI check, PR template and agent instructions for client repos live in [eimerreis/freelance-ops](https://github.com/eimerreis/freelance-ops).

## Documents

Business documents in the EimerReis brand, one YAML file each, rendered to PDF/A-3b with [Typst](https://typst.app). Templates and brand fonts ship inside eimer; on first use eimer downloads Typst 0.15.1 (checksum-verified) to `~/.local/share/eimer/tools`, unless Typst 0.15+ is already on `PATH`.

```sh
eimer doc new sender                 # ~/.config/eimer/sender.yaml: your business details, once
eimer doc new invoice RE-2026-015.yaml  # start from an example
eimer doc render RE-2026-015.yaml    # -> RE-2026-015.pdf
eimer doc render vertrag.yaml --watch   # re-render on save; --png for page previews
```

The YAML's `type:` picks the template: `invoice`, `quote`, `letter`, `reminder` (`level: 0` reminder, `1, 2, …` dunning), `contract`, `time-report`.
Text fields (`intro`, `outro`, `terms`, `body`, item `detail`) are [Typst markup](https://typst.app/docs/reference/syntax/): `*bold*`, `_italic_`, `- ` lists, `+ ` numbered paragraphs, `= Title` for a contract's next § section.
Amounts and hours are plain numbers; templates format them in German.
`small_business: true` in the sender drops VAT (§ 19 UStG); `reverse_charge: true` on an invoice applies 0 %; otherwise `vat_rate` (default 19).
`logo:` (`eimerreis`, `name`, `name-stacked`) in the sender sets the default lockup; a document's `logo:` overrides it.

## Runtime secrets

`eimer secrets` keeps an application's runtime secrets in the repo as SOPS/age-encrypted dotenv files, `secrets/<environment>.enc.env` ([devops ADR-0019](https://github.com/eimerreis/devops/blob/main/docs/adr/0019-keep-runtime-secrets-sops-encrypted-in-caller-repositories.md)). `secrets/recipients.json` names who may decrypt which environment; `.sops.yaml` is generated from it. Needs `sops` and `age` on PATH; adding a host by name also needs `op`.

```bash
eimer secrets init                                   # operator key from the local SOPS age key file; production + development
eimer secrets edit development                       # $EDITOR via sops; creates the file on first use
eimer secrets recipients add eimerdev --env development   # key from the 1Password item "eimerdev" (field "age public key"), or --key age1...
eimer secrets recipients list [--json]
eimer secrets recipients remove eimerdev             # re-encrypts, then reminds you to rotate what the host could read
eimer secrets get production DATABASE_PASSWORD
eimer secrets decrypt production -o .env             # 0600; refuses paths in the repo that git does not ignore
```

Development hosts, where agents run, should never be recipients of production.

## Product updates

`eimer product` turns Changesets changelogs into public update posts (one post per feature, like a changelog page). The CLI picks and validates; a coding agent writes the post and captures screenshots.

```bash
eimer product init      # writes .eimer/product.json + .eimer/product-update.prompt.md (commit both)
eimer product status    # user-facing changes no post covers yet (--all includes ci/chore/...)
eimer product update    # pick changes → draft post → agent prompt on the clipboard
eimer product check     # validate posts (CI); --strict fails on unpublished drafts
```

`update` lists every changelog entry not yet covered, with commit type and platform tags (from the commit's files and the `platforms` globs). Internal commit types and products with `include: false` start unchecked; you can hide them for good (`<updatesDir>/.ignored.json`). It writes `<updatesDir>/<date>-<slug>.<lang>.md` with `status: draft` and copies the rendered prompt (`--print`, `--out <file>`, `--json` instead). Posts list the change ids they cover in `changes`, so nothing else tracks what is published. Scripted: `eimer product update --select abc1234,def5678 --title "Mac app" --media screenshot`.

Config keys: `since`, `products` (`changelog`, `include`), `platforms` (tag → globs, `!` excludes), `internalTypes`, `updatesDir`, `mediaDir`, `mediaUrl`, `languages` (primary first), `capture` (platform → command, `{test}` = UI test), `promptTemplate`, `ignoreFile`. See [feature 009](docs/features/009-product-updates.md).

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
