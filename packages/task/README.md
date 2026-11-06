# @scripts/task

CLI for Azure DevOps task workflows.

## Development

```bash
bun run dev -- <command>
```

## Build

```bash
bun run build
```

Outputs global binary to `../../bin/task`.

## Commands

- `task create <title>`: Create task in the active configured team iteration + configured area path; fills Original Estimate from `--estimate` or `--remaining`
- `task list`: Alias for `task list mine`
- `task list mine`: Not-started tasks (New / To Do / Not Started / Proposed) assigned to you in the current sprint; `--all` for every state
- `task list team`: Active tasks in the team's current sprint; `--all` for every state
- `task list children <id>`: Child tasks of a parent work item (`task list --parent <id>` still works)
- `task recent`: List your recently assigned task items only
- `task start <id>`: Set task state to `Active`
- `task close <id>`: Close a task; optional `--completed <hours>` also sets Remaining Work to `0`
- `task show <id>`: Show task details, optionally with parent tree (root must be Task unless `--allow-non-task`)
- `task context show`: Show the active context and where it was resolved from
- `task context list`: List contexts defined in config.json
- `task context use <name>`: Write `.eimerrc` in the current directory; `--global` sets `defaultContext` instead

All commands accept `--context <name>`.

## Contexts

Contexts let task commands act in a specific Azure DevOps org/project regardless of the current directory's git remote. Define them in `~/.config/eimer/config.json`:

```json
{
  "defaultContext": "tapio",
  "contexts": {
    "tapio": { "org": "tapioone", "project": "tapio", "team": "My Team", "areaPath": "tapio\\My Area" },
    "homag": { "org": "homag-group", "project": "onehomag" }
  }
}
```

The active context is resolved in this order:

1. `--context <name>` flag
2. `EIMER_CONTEXT` env var
3. Nearest `.eimerrc` walking up from the current directory, e.g. `{ "context": "homag" }`
4. `defaultContext` in config.json
5. None: org/project are detected from the git remote (previous behavior)

If the active context differs from the current repo's Azure DevOps remote, a warning is printed (not with `--json`).

## Defaults

Fallback defaults from `~/.config/eimer/config.json`, used when the active context does not set `team` / `areaPath`:

- `task.defaultTeam`: replaces the built-in `Default Team` placeholder
- `task.defaultAreaPath`: replaces the built-in `Company/Engineering` area path placeholder
