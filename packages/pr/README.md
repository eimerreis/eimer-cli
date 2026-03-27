# @scripts/pr

CLI for PR workflows (Azure DevOps + GitHub detection).

## Development

```bash
bun run dev -- <command>
```

## Build

```bash
bun run build
```

Outputs global binary to `../../bin/pr`.

## Commands

- `pr create <title>`: Create PR and enable auto-complete
- `pr comments`: List review comments for current branch PR
- `pr copy`: Copy PR reference text + link to clipboard for current branch PR
- `pr show`: Show merge readiness (comments, approvals, checks, and why not ready)
- `pr open`: Open PR for current branch (prints message when none exists)
