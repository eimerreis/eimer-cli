# Feature: Unified CLI Positional Arguments

## Overview
Standardize positional argument support across all CLI commands. Currently around half the commands accept positional args while the rest require flags, creating an inconsistent UX. This feature adds positional args to the remaining commands, extracts shared ID/string resolution helpers to reduce duplication, and aligns usage error messages with actual behavior.

## User Stories
- As a developer, I want to type `pr show 42` instead of `pr show --id 42` so I can work faster.
- As a developer, I want to type `pipeline trigger deploy-prod` instead of `pipeline trigger --name deploy-prod` so the most common argument is frictionless.
- As a developer, I want to type `release changelog deploy-prod` instead of `release changelog --pipeline deploy-prod` because pipeline name is the primary argument.
- As a developer, I want consistent error messages that show `Usage: task start [id]` instead of `Usage: task start --id <number>` so I know positional args work.

## Technical Approach

### Positional arg convention
- ID-taking commands: parse `positional[0]` as a positive integer; prefer explicit flag override; fall back to auto-detect or interactive prompt where already supported.
- Name-taking commands: use `positional[0]` or `positional.join(" ")` as string; prefer explicit flag override; fall back to existing prompt/default lookup behavior.
- Keep all existing flags (`--id`, `--name`, `--pipeline`, etc.) unchanged for scripting compatibility.

### Shared helpers (per-package utils)
Add helper functions in existing per-package utility modules (`comments-utils.ts` for pr, `utils.ts` for others):

```ts
function resolveIdArg(flag: number | undefined, positional: string[]): number | undefined {
  if (flag) return flag;
  if (positional.length > 0) {
    const parsed = Number.parseInt(positional[0], 10);
    if (Number.isFinite(parsed) && parsed > 0) return parsed;
  }
  return undefined;
}

function resolveStringArg(flag: string | undefined, positional: string[]): string {
  if (flag?.trim()) return flag.trim();
  return positional.join(" ").trim();
}
```

### Commands to add positional support
- `pr comments` -> PR ID (int)
- `pr open` -> PR ID (int)
- `pr show` -> PR ID (int)
- `pr copy` -> PR ID (int)
- `pipeline trigger` -> pipeline name (string)
- `release changelog` -> pipeline name (string)
- `release approve` -> run ID (int)
- `task list` -> parent ID (int)

### Commands to refactor (already support positional)
Refactor to use shared parser helpers and standardize messages:
- `task start`, `task close`, `task show`
- `pipeline show`, `pipeline open`, `pipeline watch`

## Implementation Plan
- [x] Add positional args to PR commands and standardize usage/error messaging.
- [x] Add positional args to pipeline trigger and refactor existing pipeline ID commands to shared helpers.
- [x] Add positional args to release changelog and release approve, including helper extraction.
- [x] Add positional arg support to task list and refactor task start/close/show to shared helpers.

## Dependencies
No external dependencies. This is an additive refactor across existing command files and package-level utils.

## Acceptance Criteria
- [ ] `pr comments 42`, `pr open 42`, `pr show 42`, and `pr copy 42` work.
- [ ] `pipeline trigger deploy-prod` works.
- [ ] `release changelog deploy-prod` works.
- [ ] `release approve 12345` works.
- [ ] `task list 12345` works.
- [ ] Existing flag-based usage continues to work unchanged.
- [ ] Usage/error messages consistently reflect positional support (`Usage: <cmd> [arg]`).
- [ ] Existing positional commands use shared parser helper(s) instead of ad-hoc parsing.
