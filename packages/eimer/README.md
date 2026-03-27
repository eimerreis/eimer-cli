# @scripts/eimer

Meta CLI for the scripts monorepo.

## Development

```bash
bun run dev -- <command>
```

## Build

```bash
bun run build
```

Outputs global binary to `../../bin/eimer`.

## Commands

- `eimer pr ...`: Run PR commands
- `eimer pipeline ...`: Run pipeline commands
- `eimer release ...`: Run release commands
- `eimer task ...`: Run task commands
- `eimer configure`: Manage shared defaults in `~/.config/eimer/config.json`
