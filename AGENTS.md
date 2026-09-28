# Eimer Scripts

Personal developer CLI toolkit - automates Azure DevOps and GitHub workflows (PRs, pipelines, releases, tasks) from the terminal.

## Docs
- [Product](docs/product.md)
- [Tech Stack](docs/tech-stack.md)
- [Architecture](docs/architecture.md)
- [Feature Specs](docs/features/)
- [README / Quick start](README.md)

## Setup

```bash
./install.sh            # fresh machine: installs Bun if missing, then `bun run setup`
bun run setup           # install deps, build bin/eimer, link into ~/.local/bin, run `eimer doctor`
```

## Build

```bash
bun run build           # bin/eimer (contains all commands)
bun run build:all       # also bin/pr, bin/pipeline, bin/release, bin/task
cd packages/<name> && bun run build   # single package
```

Builds use `bun build --compile` (not `bunli build`).

## Dev

```bash
bun run dev -- <command>                       # eimer from source
cd packages/<name> && bun run dev -- <command> # a single package
```

## Typecheck & Test

```bash
bun run typecheck
bun run test
cd packages/<name> && bun test
```

## Dependencies
- `bunli`, `@bunli/*` are pinned to exact versions and `@opentui/core`/`@opentui/react` are forced via root `overrides`: two copies of `@opentui/core` crash at startup. Upgrade them together.
- `.bunli/commands.gen.ts` files are committed; refresh with `bun run generate` (no postinstall hook).

## Coding Conventions
- **Language**: TypeScript (strict mode, ES2022, ESNext modules)
- **Runtime**: Bun - use `Bun.spawn`, `Bun.file`, `Bun.write` directly; never use Node/npm
- **CLI framework**: Bunli - `defineCommand`, `option`, `createCLI` from `@bunli/core`
- **Validation**: Zod v4 for schemas and option types
- **Modules**: ESM only (`"type": "module"`)
- **Package manager**: Bun only - `bun install`, `bun run`, `bun test`, `bun run --filter`
- **Commits**: Conventional Commits (`feat|fix|refactor|build|ci|chore|docs|style|perf|test`)
- **File size**: Keep files under ~500 LOC; split/refactor as needed
- **Command structure**: Each command in its own file under `src/commands/`, default-exported
- **Boolean options**: always pass `argumentKind: "flag"` to `option(z.coerce.boolean()...)`, otherwise Bunli parses a bare `--flag` as `false` (guarded by `packages/eimer/src/lib/flags.test.ts`)
- **Subprocesses/URLs**: use `runText`/`runJson`/`openUrl`/`requireTool` from `@scripts/ui` (clear errors when `az`/`gh` is missing, cross-platform browser opening)
- **Exports**: Domain packages export commands via `"./commands"` entry point for composition in eimer
