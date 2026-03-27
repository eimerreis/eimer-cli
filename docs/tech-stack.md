# Tech Stack

## Overview
TypeScript monorepo of CLI tools running on Bun, built with Bunli (a Bun-native CLI framework), compiled to native binaries.

## Choices

### Runtime
- **Bun** - Primary runtime and package manager. All packages use `bun run`, `bun test`, and Bun APIs directly (`Bun.file`, `Bun.write`, `Bun.env`).

### Language
- **TypeScript** - Strict mode, ES2022 target, ESNext modules, bundler module resolution. All source is `.ts`.

### CLI Framework
- **Bunli** (`@bunli/core`, `bunli` CLI) - Bun-native CLI framework. Handles command definition, argument parsing, code generation (`bunli generate`), and native binary compilation (`bunli build --native`).

### Monorepo Structure
- **npm workspaces** - `"workspaces": ["packages/*"]` in root `package.json`. Seven packages: `eimer`, `pr`, `pipeline`, `release`, `task`, `config`, `helpers`.

### Schema Validation
- **Zod v4** - Config schema validation and CLI argument types.

### Build & Distribution
- **`bunli build --native`** - Compiles each CLI to a standalone native binary in `bin/` (eimer, pr, pipeline, release, task).
- **No bundler** - Bunli handles compilation; no Vite/esbuild/webpack.

### Testing
- **`bun test`** - Built-in Bun test runner. `@bunli/test` available as dev dependency.

### Type Checking
- **`tsc --noEmit`** - Per-package typecheck via workspace scripts.

### External APIs
- **Azure DevOps REST API** - PRs, pipelines, work items, approvals (via `az` CLI subprocess)
- **GitHub API** - PR comments support for GitHub repos
- **Microsoft Teams Webhooks** - Changelog posting via Adaptive Card / MessageCard payloads

### Helpers Package (standalone)
- **tsx** - Used in `@scripts/helpers` for ad-hoc scripts
- **execa** - Process execution in helpers
- **cmd-ts** - Alternative CLI framework used only in helpers (legacy/standalone)
- **glob** - File pattern matching in helpers

### Key Libraries
| Library | Purpose | Why |
|---------|---------|-----|
| `@bunli/core` | CLI framework | Bun-native, handles commands + args + native compilation |
| `zod` v4 | Schema validation | Config validation, type-safe parsing |
| `execa` | Child process execution | Used in helpers for external tool invocation |
| `tsx` | TypeScript execution | Runs ad-hoc helper scripts outside Bun context |
