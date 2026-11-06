# Feature: Task Contexts & az-style Command Structure

## Overview
The task CLI currently derives the Azure DevOps org/project from the git remote of the current directory (`--detect true`) and selects list modes via flags (`--parent`, `--team`). This feature introduces **named contexts** (e.g. `tapio` → `tapioone/tapio`, `homag` → `homag-group/onehomag`) selected via a small `.eimerrc` file, adds a `task list mine` view for not-started tasks assigned to me, and restructures `task list` into az-cli-style subcommands.

## User Stories
- As a developer working for multiple clients, I want to drop a `.eimerrc` into a folder so every `task` command run below it targets the right Azure DevOps project — even outside a git repo.
- As a developer, I want `task list mine` to show my not-started tasks in the current sprint so I know what to pick up next.
- As a developer, I want `task list team` / `task list mine` / `task list children <id>` instead of mode flags, like `az boards ...`.
- As a developer, I want `task context show` to tell me which context is active and why.

## Technical Approach

### Context definitions (config.json)
Contexts live in `~/.config/eimer/config.json` (not hardcoded — this repo is public):

```json
{
  "defaultContext": "tapio",
  "contexts": {
    "tapio": { "org": "tapioone", "project": "tapio", "team": "...", "areaPath": "..." },
    "homag": { "org": "homag-group", "project": "onehomag", "team": "...", "areaPath": "..." }
  }
}
```

- `org` and `project` required; `team` and `areaPath` optional.
- Per-context `team`/`areaPath` supersede the global `task.defaultTeam`/`task.defaultAreaPath`, which remain as fallback for backward compatibility.

### `.eimerrc` file
Minimal JSON, validated with Zod: `{ "context": "homag" }`. Discovered by walking up from `cwd` to `/`; nearest file wins.

### Resolution order
1. `--context <name>` flag (global on all task commands)
2. `EIMER_CONTEXT` env var
3. Nearest `.eimerrc` (walking up from cwd)
4. `defaultContext` in config.json
5. No context → legacy behavior (`--detect true` from git remote)

Unknown context name → hard error listing the defined contexts.

**Conflict with git remote:** if a context is resolved and the cwd's ADO remote points to a different org/project, the context wins and a one-line warning is printed (suppressed with `--json`).

### Shared infrastructure (`@scripts/config`)
Resolution lives in `@scripts/config` so pr/pipeline/release can opt in later; **only task is wired up in this feature**.
- `resolveContext({ flag }): Promise<ResolvedContext | null>` → `{ name, org, project, team?, areaPath?, source: "flag" | "env" | "rc" | "global" }`
- `findRcFile(cwd)`, `writeRcFile(dir, context)`
- `initContext(flag)` in task utils resolves the context once per invocation; `azScopeArgs()`: returns `["--org", "https://dev.azure.com/<org>", "--project", "<project>"]` when a context is set, else `["--detect", "true"]`. All `az` invocations in task use it.
- `buildWorkItemUrl` / `tryGetAzureContext` prefer the resolved context over the git remote.

### Command structure
```
task list               → alias for `task list mine`
task list mine          → not-started tasks assigned to @Me in current sprint
task list team          → active tasks in team's current sprint (old default)
task list children <id> → child tasks of a parent item (old --parent)
task context show       → active context + source
task context list       → defined contexts (marks active)
task context use <name> → write .eimerrc in cwd; --global sets defaultContext
```
- Filters stay flags (`--team`, `--all`, `--top`, `--json`) — only mode selection moves to subcommands.
- `task list --parent <id>` (and `task list <id>`) stay as backward-compatible aliases for `children`; Bunli has no hidden options, so `--parent` is marked deprecated in help.
- Implemented with Bunli nested command groups (`{ name, commands: [...] }`, as used in `eimer`). `eimer task ...` gets the same structure.

### `task list mine` query
- Iteration: current iteration of the context's team (`loadCurrentIterationByTeam`), `--team` overrides.
- WIQL: `[System.WorkItemType] = 'Task' AND [System.AssignedTo] = @Me AND [System.IterationPath] = '<path>' AND [System.State] IN ('New', 'To Do', 'Not Started', 'Proposed')`
- `--all` drops the state filter (all my tasks in the sprint).
- Fixed built-in state set covers both process templates; no per-context config.

## Implementation Plan
- [x] Extend config schema with `defaultContext` and `contexts`; add `resolveContext`, rc discovery/writing to `@scripts/config` with unit tests for precedence.
- [x] Add `azScopeArgs` to task utils and replace all `--detect true` usages; make work-item URLs context-aware; add mismatch warning.
- [x] Add global `--context` option to all task commands.
- [x] Split `list.ts` into `list/mine.ts`, `list/team.ts`, `list/children.ts` (+ shared list helpers); wire as nested group with bare `list` → `mine` and hidden `--parent` alias.
- [x] Add `task context show|list|use` commands.
- [x] Register new structure in `packages/task/src/index.ts` and `packages/eimer/src/index.ts`.
- [x] Update docs (architecture config section, product command list).

### Incidental fixes
- Bunli 0.9 requires `argumentKind: "flag"` for bare boolean flags (`--json`, `--all`); without it they silently parse as `false`. Added to all task options.
- `@scripts/ui` `renderTable` crashed when `colWidths`/`colAligns` were undefined (cli-table3 0.6.5).
- `eimer configure` dropped unknown top-level keys on save; it now preserves `contexts` and `defaultContext`.

## Dependencies
- Bunli nested command groups (already used by `@scripts/eimer`).
- `az` CLI `--org`/`--project` flags on `az boards` commands.
- Builds on `002-unified-positional-args.md` (positional `<id>` for `children`).

## Acceptance Criteria
- [x] With `.eimerrc` `{ "context": "homag" }` in a parent dir, `task list mine` queries `homag-group/onehomag` from any subdirectory, including non-git dirs.
- [x] `--context` overrides env, env overrides rc, rc overrides global default.
- [x] No context configured → behavior identical to today.
- [x] `task list` and `task list mine` return only not-started tasks assigned to me in the current sprint.
- [x] `task list team` matches the old `task list` output; `task list --parent 123` still works.
- [x] `task context show` prints the context name, org/project, and resolution source.
- [x] Context vs. git-remote mismatch prints a warning (not in `--json` mode).
