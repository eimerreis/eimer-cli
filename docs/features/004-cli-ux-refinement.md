# Feature: CLI UX Refinement (gh-style)

## Overview
Refine the human-readable UX of the Eimer CLI to feel much closer to GitHub CLI: scannable status symbols, clean bordered tables, consistent success and error output, and visible progress during slow operations. This should improve daily usability across all user-facing commands without changing command semantics or `--json` output.

## User Stories
- As a developer, I want colored status symbols and consistent labels so I can scan PR, pipeline, task, and release output quickly.
- As a developer, I want list-style commands to render as aligned bordered tables so multi-column output is easier to read.
- As a developer, I want progress spinners for slow Azure DevOps and GitHub operations so I know the CLI is working instead of hanging.
- As a developer, I want clearer empty states and error hints so I know what to do next when a command returns no data or fails.

## Technical Approach
Introduce a shared `@scripts/ui` workspace package for human-readable CLI presentation. It should provide chalk-based color helpers, status symbols, bordered table rendering via `cli-table3`, an `ora`-based spinner wrapper, and shared formatting helpers such as terminal links and relative time.

Apply the package across all user-facing commands in `@scripts/pr`, `@scripts/pipeline`, `@scripts/task`, `@scripts/release`, and the composed `@scripts/eimer` experience. Follow GitHub CLI conventions closely where they fit the existing command model, but do not change command names, flags, response semantics, or machine-readable `--json` output.

Key design points:
- Human-readable output only; `--json` stays raw and unformatted.
- Prefer consistent patterns across packages over per-command customization.
- Replace duplicated formatting helpers with shared utilities from `@scripts/ui`.
- Preserve non-TTY behavior by relying on libraries that gracefully disable color/spinners where appropriate.

## Implementation Plan
- [x] Create `@scripts/ui` as a shared workspace package with `chalk`, `cli-table3`, and `ora`; export status symbols, text styling helpers, bordered table helpers, structured success/error/warning printers, terminal link helpers, relative time helpers, and a spinner wrapper for async operations.
- [x] Migrate `@scripts/pr` human-readable output to `@scripts/ui`, including list/show/comments flows, GitHub/Azure status presentation, structured success/error output, and spinner-wrapped network or subprocess operations.
- [x] Migrate `@scripts/pipeline` human-readable output to `@scripts/ui`, including run status symbols, bordered run listings, watch/show feedback, and progress indicators during polling or lookup operations.
- [x] Migrate `@scripts/task` human-readable output to `@scripts/ui`, including task state presentation, task list/recent table output, clear empty states, and structured mutation feedback for create/start/close flows.
- [x] Migrate `@scripts/release` human-readable output to `@scripts/ui`, including approval status output, changelog flow messaging, webhook/posting feedback, and spinners for slower operations.
- [x] Remove duplicated formatting utilities across packages, update imports to use `@scripts/ui`, and align `eimer` top-level command output with the same conventions.
- [x] Add UX copy pass for empty states, hints, and actionable follow-ups so commands suggest next flags or likely fixes when no results are found.

## Dependencies
- Existing command architecture in `@scripts/pr`, `@scripts/pipeline`, `@scripts/task`, `@scripts/release`, and `@scripts/eimer`
- Current Bun/Bunli monorepo workspace setup
- New workspace dependencies: `chalk`, `cli-table3`, `ora`
- Awareness of feature `002-unified-positional-args.md` to avoid overlapping command-output edits in the same files

## Acceptance Criteria
- [ ] Human-readable status indicators use consistent colored symbols instead of ad hoc bracketed labels where applicable.
- [ ] List-oriented commands render as aligned bordered tables across `pr`, `pipeline`, and `task` flows.
- [ ] Slow subprocess/API operations show a spinner or equivalent progress feedback in human-readable mode.
- [ ] Success, warning, and error output is consistently structured across packages.
- [ ] Empty states include actionable suggestions where relevant.
- [ ] Existing command behavior, flags, and data semantics remain unchanged.
- [ ] `--json` output remains unstyled and backward-compatible.
- [ ] Duplicated formatting helpers are removed or consolidated behind `@scripts/ui`.
