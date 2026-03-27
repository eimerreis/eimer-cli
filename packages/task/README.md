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
- `task list`: List tasks in the active team sprint (Task items only) or child tasks for a parent (`--parent <id>`)
- `task recent`: List your recently assigned task items only
- `task start <id>`: Set task state to `Active`
- `task close <id>`: Close a task; optional `--completed <hours>` also sets Remaining Work to `0`
- `task show <id>`: Show task details, optionally with parent tree (root must be Task unless `--allow-non-task`)

Shared defaults from `~/.config/eimer/config.json`:

- `task.defaultTeam`: replaces the built-in `Default Team` placeholder
- `task.defaultAreaPath`: replaces the built-in `Company/Engineering` area path placeholder
