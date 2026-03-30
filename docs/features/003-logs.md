# Feature: App Insights Log Viewer

## Overview
New `@scripts/logs` package adding `eimer logs list`, `eimer logs show`, and `eimer logs open` commands. Queries Azure Application Insights via the `az monitor app-insights` CLI to surface recent log entries (exceptions, requests, traces, dependencies, etc.) directly in the terminal - eliminating the need to open the Azure portal for routine error checking.

## User Stories
- As a developer, I want to see recent exceptions from my app without opening the Azure portal so I can spot errors quickly during development
- As a developer, I want to filter log entries by type (exceptions, requests, traces, dependencies) so I can focus on what matters
- As a developer, I want to see the full stack trace and custom dimensions of a specific log entry so I can debug issues in context
- As a developer, I want to jump to the App Insights portal with one command so I can drill deeper when the CLI is not enough

## Technical Approach

### API Layer
Primary: `az monitor app-insights events show --app <app> --type <type>` for type-filtered event listing. Returns JSON with event details, timestamps, severity levels.

Future extension: `az monitor app-insights query --app <app> --analytics-query <kql>` for custom KQL queries.

### App Resolution (precedence order)
1. `--app` CLI flag (explicit override)
2. `.eimer.json` in git repo root -> `logs.app` (per-repo config)
3. `~/.config/eimer/config.json` -> `logs.defaultApp` (global fallback)
4. Error with helpful message explaining how to configure

### Repo-Level Config (new pattern)
Introduces `.eimer.json` at the repository root - a new config layer for per-repo settings. The `@scripts/config` package gains a `loadRepoConfig()` function that locates the git root and reads this file. This pattern can be reused by other packages in the future.

```jsonc
// .eimer.json (repo root)
{
  "logs": {
    "app": "my-app-insights-resource-name"
  }
}
```

### Extension Auto-Detection
On first use, the CLI checks whether the `application-insights` az CLI extension is installed (`az extension list`). If missing, it prompts the user to install it automatically (`az extension add --name application-insights`). If the user declines, the CLI exits with a clear error message and the manual install command.

### Commands
- **`logs list`** - Recent events with filters: `--type` (exceptions/requests/traces/dependencies/customEvents/availabilityResults), `--top` (count), `--timespan` (e.g. PT1H, P1D, P7D), `--json` for machine output. Color-coded severity in terminal.
- **`logs show <id>`** - Full detail of a single event by ID. Stack traces for exceptions, request URL/status/duration for requests, custom dimensions, operation context.
- **`logs open`** - Opens App Insights portal in browser for the configured app. Constructs portal URL from app resource.

### Package Structure
New `packages/logs/` workspace package following existing conventions:
- `src/commands/list.ts` - list command
- `src/commands/show.ts` - show command
- `src/commands/open.ts` - open command
- `src/commands/utils.ts` - runJson/runText helpers, type definitions, app resolution, extension detection
- Exports commands via `"./commands"` entry point

### Affected Components
- **`@scripts/config`** - new `logs` section in global schema, new `loadRepoConfig()` function
- **`@scripts/eimer`** - register `logs` command group in meta-CLI
- **`@scripts/logs`** - entirely new package

## Implementation Plan
- [ ] Create `@scripts/logs` package scaffold - package.json, tsconfig.json, src/commands/utils.ts with runJson/runText helpers, App Insights event type definitions (exceptions, requests, traces, etc.), app resolution logic (--app flag -> repo config -> global config -> error), extension auto-detection with install prompt
- [ ] Extend `@scripts/config` with repo-level config support - add `loadRepoConfig()` that finds git root and reads `.eimer.json`, add `logs.defaultApp` to global config schema, export merge utility (repo config overrides global config)
- [ ] Implement `logs list` command - call `az monitor app-insights events show`, support `--type`, `--top`, `--timespan` filters, formatted terminal output with color-coded severity and relative timestamps, `--json` flag for machine-readable output
- [ ] Implement `logs show` command - fetch single event by ID via `az monitor app-insights events show --event <id>`, display full detail (stack trace for exceptions, request URL/status/duration for requests, custom dimensions, operation ID)
- [ ] Implement `logs open` command - construct Azure Portal App Insights URL from resolved app name/resource, open in default browser via `open` command
- [ ] Register in eimer meta-CLI - import commands from `@scripts/logs/commands`, add `logs` command group to `packages/eimer/src/index.ts`, add `@scripts/logs` as workspace dependency of `@scripts/eimer`

## Dependencies
- `az` CLI with Application Insights extension (`az extension add --name application-insights`) - auto-detected and prompted
- Azure subscription with Application Insights resource(s) provisioned
- Existing `@scripts/config` package (extended in this feature)

## Acceptance Criteria
- [ ] `eimer logs list` shows recent log entries from configured App Insights app
- [ ] `eimer logs list --type exceptions` filters to exceptions only
- [ ] `eimer logs list --type requests --top 5` limits results and filters by type
- [ ] `eimer logs list --timespan P1D` restricts to last 24 hours
- [ ] `eimer logs show <id>` displays full event detail including stack trace for exceptions
- [ ] `eimer logs open` opens App Insights portal in default browser
- [ ] App resolution: `--app` flag > `.eimer.json` in repo root > global config > clear error message
- [ ] `.eimer.json` with `logs.app` is picked up automatically when running from a repo
- [ ] `eimer logs list --json` outputs machine-readable JSON
- [ ] Missing `application-insights` extension is detected and user is prompted to install it
- [ ] Works as standalone `logs` binary via `bunli build --native` and via `eimer logs` meta-command
