# Eimer Scripts

Personal developer CLI toolkit - automates Azure DevOps and GitHub workflows (PRs, pipelines, releases, tasks) from the terminal.

## Docs
- [Product](docs/product.md)
- [Tech Stack](docs/tech-stack.md)
- [Architecture](docs/architecture.md)
- [Feature Specs](docs/features/)

## Build

```bash
# Build all CLI binaries (from root)
bun run --filter '@scripts/eimer' build
bun run --filter '@scripts/pr' build
bun run --filter '@scripts/pipeline' build
bun run --filter '@scripts/release' build
bun run --filter '@scripts/task' build

# Build a single package directly
cd packages/<name> && bun run build
```

## Dev

```bash
# Run a package in dev mode
cd packages/<name> && bun run dev -- <command>
```

## Typecheck

```bash
# Per-package
bun run --filter '@scripts/eimer' typecheck
bun run --filter '@scripts/pr' typecheck
bun run --filter '@scripts/pipeline' typecheck
bun run --filter '@scripts/release' typecheck
bun run --filter '@scripts/task' typecheck
bun run --filter '@scripts/config' typecheck
```

## Test

```bash
cd packages/<name> && bun test
```

## Coding Conventions
- **Language**: TypeScript (strict mode, ES2022, ESNext modules)
- **Runtime**: Bun - use `Bun.spawn`, `Bun.file`, `Bun.write` directly; never use Node/npm
- **CLI framework**: Bunli - `defineCommand`, `option`, `createCLI` from `@bunli/core`
- **Validation**: Zod v4 for schemas and option types
- **Modules**: ESM only (`"type": "module"`)
- **Package manager**: Bun only - `bun install`, `bun run`, `bun test`, `bun run --filter`
- **Commits**: Conventional Commits (`feat|fix|refactor|build|ci|chore|docs|style|perf|test`)
- **File size**: Keep files under ~500 LOC; split/refactor as needed
- **Subprocess pattern**: `Bun.spawn` with stdout/stderr pipe -> await `.exited` -> parse
- **Command structure**: Each command in its own file under `src/commands/`, default-exported
- **Exports**: Domain packages export commands via `"./commands"` entry point for composition in eimer

## Pending Migrations
- Root `package.json` scripts still use `npm --workspace` - migrate to `bun run --filter` or direct `cd packages/<name> && bun run` pattern
