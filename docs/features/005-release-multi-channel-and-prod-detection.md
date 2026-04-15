# Feature: Release Multi-Channel & Prod Stage Detection

## Overview
Make `@tapio/release` usable by teams beyond the original author by supporting multiple named Teams webhook channels (with interactive selection) and auto-detecting/configuring the production stage name instead of relying on hardcoded "prod" heuristics.

## User Stories
- As a **new release CLI user**, I want a `release configure` wizard so I can set up my pipeline, Teams channels, and prod stage name without editing JSON by hand.
- As a **team posting changelogs to different channels**, I want to select which Teams channel to post to at runtime so I can target the right audience per release.
- As a **user with non-standard prod stage naming**, I want the CLI to auto-detect or let me configure my prod stage name so changelog resolution works without manual `--from` overrides.

## Technical Approach

### Config schema expansion (`config.ts`)

Current config supports a single `teams.webhookUrl`. This feature expands it to support named channels while maintaining backward compatibility:

```json
{
  "teams": {
    "webhookUrl": "https://...",
    "channels": {
      "frontend-releases": "https://webhook1...",
      "backend-releases": "https://webhook2..."
    }
  },
  "release": {
    "defaultPipeline": "my-pipeline",
    "prodStageName": "deploy_production"
  }
}
```

- `teams.webhookUrl` (existing) — treated as a channel named "default" for backward compat
- `teams.channels` (new) — named map of channel label → webhook URL
- `release.prodStageName` (new) — explicit prod stage identifier/name override

### Affected components

| File | Change |
|------|--------|
| `packages/release/src/config.ts` | Schema expansion, backward compat helper `resolveChannels()` |
| `packages/release/src/commands/changelog.ts` | Channel picker integration, prod stage config passthrough |
| `packages/release/src/commands/configure.ts` | **New** — interactive setup wizard |
| `packages/release/src/commands/index.ts` | Register `configure` command |

`teams-webhook.ts` requires no changes — it already accepts a webhook URL as parameter.

### Key design decisions

1. **Backward compat**: Existing single `webhookUrl` continues to work unchanged. Internally merged into channels map as "default".
2. **Channel resolution order**: `--post-webhook` flag → `--channel` flag (picks from config) → interactive picker (if multiple channels) → auto-select (if single channel)
3. **Prod stage detection**: Config `prodStageName` takes priority → expanded heuristics as fallback (add "production", "release", "live" patterns beyond current "prod") → error with suggestion to run `release configure`
4. **`release configure`**: Introspects pipeline timeline to list actual stage names for prod stage selection — no guessing required

## Implementation Plan
- [x] **Expand config schema** — Add `teams.channels` (record of name → URL) and `release.prodStageName` (string) to Zod schema in `config.ts`. Add `resolveChannels()` helper that merges legacy `webhookUrl` into channels map as "default". All existing configs must continue to parse without error.
- [x] **Refactor prod stage detection** — Extract stage matching from `hasSuccessfulProdStage()` into configurable function. Read `prodStageName` from config. Expand heuristics to also match "production", "live", "release" patterns. Add `listPipelineStages()` utility that returns all stage records from a build timeline for use in the configure wizard.
- [x] **Add channel picker to changelog command** — Add `--channel` CLI flag. Replace inline `webhookUrl` resolution with: if `--post-webhook` set → use directly; elif `--channel` set → look up from resolved channels; elif multiple channels → show interactive picker via `prompt.select`; elif single channel → use it. Update Teams post success message to include channel name.
- [x] **Build `release configure` wizard** — New `configure.ts` command: prompts for default pipeline name, uses `resolvePipelineByName()` to validate, introspects pipeline timeline via `listPipelineStages()` to show stage names for prod stage selection, prompts for Teams channels in add-loop (name + URL pairs). Writes `~/.config/tapio-release/config.json`. Register in `commands/index.ts`.
- [x] **Add `release configure --show`** — Add `--show` flag to the configure command. When set, loads and pretty-prints current config: default pipeline, prod stage name, all configured channels (name + masked URL). No prompts, display-only.

## Dependencies
- No new external dependencies
- Requires existing `az pipelines` and timeline API access (already in place)
- No blocking dependencies on other features

## Acceptance Criteria
- [x] Existing single-webhook configs continue to work without any changes
- [x] `release configure` walks user through pipeline, prod stage, and channels setup and writes valid config
- [x] `release changelog --channel frontend-releases` posts to the named channel without prompting
- [x] `release changelog` with multiple configured channels shows interactive picker
- [x] `release changelog --post-webhook <url>` still works as ad-hoc override, bypassing config
- [x] Prod stage detection uses config `prodStageName` when set, falls back to expanded heuristics
- [x] Pipeline with non-standard prod stage name (e.g. "deploy_production") is correctly detected after running `release configure`
- [x] `release configure --show` displays current config in human-readable format
- [x] Config schema validates cleanly with Zod — invalid configs produce clear error messages
