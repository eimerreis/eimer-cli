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
- `eimer release configure`: Release defaults in `~/.config/tapio-release/config.json`
- `eimer configure`: Task defaults in `~/.config/eimer/config.json` (`--show` prints both files)
- `eimer doctor`: Check tools, logins, config and PATH
- `eimer update`: `git pull`, `bun install`, rebuild (`--no-pull`, `--all`)
- `eimer completions <zsh|bash|fish>`: Print shell completion script

See the [root README](../../README.md) for installation.
