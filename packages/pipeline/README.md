# @scripts/pipeline

CLI for Azure DevOps pipeline workflows.

## Development

```bash
bun run dev -- <command>
```

## Build

```bash
bun run build
```

Outputs global binary to `../../bin/pipeline`.

## Commands

- `pipeline runs`: List recent runs for current repo (clickable links to Azure DevOps)
- `pipeline list [glob...]`: List pipelines, optionally filtered by glob patterns (for example `tapio.Twinio.*`)
- `pipeline open [id]`: Open latest run (or a specific run by ID) in browser
- `pipeline show <id>`: Show detailed metadata for one run
- `pipeline trigger`: Queue a new pipeline run (`--id`, `--name`, `--branch`, `--parameter`, `--variable`)
- `pipeline watch [id]`: Poll run status until completion (uses latest repo run when ID omitted)
