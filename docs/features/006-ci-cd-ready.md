# Feature: CI/CD-Ready Release CLI

## Overview
Eliminate the Azure CLI dependency for CI/CD environments by adding a direct HTTP client for Azure DevOps REST API calls. The release CLI resolves auth from pipeline environment variables (`SYSTEM_ACCESSTOKEN`) and falls back to `az` CLI for local interactive use. This makes `bunx @tapio/release changelog` work in Azure Pipelines with zero tool installation beyond Bun.

## User Stories
- As a **pipeline operator**, I want to run `bunx @tapio/release changelog --pipeline "MyApp" --post-webhook $TEAMS_WEBHOOK` in a post-deployment step so release notes are auto-posted without installing `az` CLI.
- As a **local developer**, I want existing `az`-based auth to keep working so I don't need to change my workflow.
- As a **DevOps engineer**, I want the CLI to auto-detect Azure Pipeline context (`SYSTEM_COLLECTIONURI`, `SYSTEM_TEAMPROJECT`, `SYSTEM_ACCESSTOKEN`) so I don't need to pass org/project manually.

## Technical Approach

### Auth resolution order
1. `SYSTEM_ACCESSTOKEN` env var (Azure Pipelines built-in OAuth token)
2. `AZURE_DEVOPS_PAT` env var (manual PAT for other CI systems)
3. Fall back to `az` CLI subprocess (existing local behavior)

### Context resolution order
1. `SYSTEM_COLLECTIONURI` + `SYSTEM_TEAMPROJECT` env vars → construct base URL
2. Fall back to `az devops configure --list` (existing local behavior)

### New `AzureDevOpsClient` class (`src/azure-client.ts`)
- Constructor: `baseUrl` + `token`
- `get<T>(path, params?)` — GET with query params, parse JSON
- `patch<T>(path, body)` — PATCH with JSON body, parse response
- Auth header injection: `Authorization: Bearer {token}`
- Error handling: HTTP status → descriptive errors

### API endpoint mapping (replacing `az` CLI calls)

| Current `az` call | REST endpoint |
|---|---|
| `az pipelines list --name X` | `GET /_apis/build/definitions?name={X}&$top=20&api-version=7.1` |
| `az pipelines runs show --id X` | `GET /_apis/build/builds/{X}?api-version=7.1` |
| `az pipelines runs list ...` | `GET /_apis/build/builds?definitions={id}&$top={n}&queryOrder=queueTimeDescending&api-version=7.1` |
| `az rest .../_apis/build/builds/{id}/timeline` | `GET /_apis/build/builds/{id}/timeline?api-version=7.1` |
| `az rest .../_apis/git/repositories/{repoId}/commits` | `GET /_apis/git/repositories/{repoId}/commits?...&api-version=7.1` |

### Affected components

| File | Change |
|------|--------|
| `src/azure-client.ts` | **New** — HTTP client, auth resolution, context resolution |
| `src/commands/utils.ts` | Refactor all `az` subprocess calls to use client |
| `src/commands/changelog.ts` | Replace `az rest` commit calls with client |
| `src/commands/prod-stage.ts` | Replace `az rest` timeline call with client |
| `src/commands/approvals.ts` | Replace `az rest` approval calls with client (free win) |

### CI mode behavior
When env vars detected (or no TTY), changelog must work fully headlessly:
- `--pipeline` required (or from config `release.defaultPipeline`)
- `--post-webhook` or `--channel` required (no interactive picker)
- Clipboard skip automatic (no TTY)
- Auto-select latest run with successful prod stage if no `--from`
- Clear error messages for missing flags

## Implementation Plan
- [x] **Create `AzureDevOpsClient` + auth/context resolution** — New `src/azure-client.ts`: HTTP client class with `get<T>`/`patch<T>`, `resolveAuth()` (env vars → az fallback), `resolveContext()` (env vars → az fallback), `createClient()` factory. Unit-testable with no external deps.
- [x] **Migrate all API calls from `az` CLI to client** — Refactor `utils.ts` (`getAzureContext`, `resolvePipelineByName`, `loadRunById`, `loadPipelineRuns`), `changelog.ts` (commit fetching), `prod-stage.ts` (timeline), and `approvals.ts` (pending/approve) to use `AzureDevOpsClient`. Remove `runJson`/`runText` calls for Azure APIs; keep only for non-Azure subprocesses if any remain.
- [x] **Harden changelog for non-interactive CI execution** — Detect CI via env vars + TTY check. Require `--pipeline` + webhook flag in CI mode. Auto-select latest prod-succeeded run when no `--from`. Skip clipboard, review, and interactive pickers. Produce clear errors when required flags are missing.
- [x] **CI documentation + pipeline template** — Add CI usage section to README: env var reference table, example Azure Pipeline YAML (post-prod step using `SYSTEM_ACCESSTOKEN`), troubleshooting guide. Update feature spec with final implementation notes.

## Dependencies
- Feature 005 (multi-channel + prod detection) — complete ✅
- Feature 001 (publish to Azure Artifacts) — complete ✅
- No new runtime dependencies (uses native `fetch()`)

## Acceptance Criteria
- [ ] `SYSTEM_ACCESSTOKEN` + `SYSTEM_COLLECTIONURI` + `SYSTEM_TEAMPROJECT` env vars provide full auth without `az` CLI
- [ ] `AZURE_DEVOPS_PAT` env var works as alternative auth for non-Azure-Pipeline CI
- [ ] When neither env var is set, falls back to `az` CLI — existing local workflow unchanged
- [ ] `bunx @tapio/release changelog --pipeline "X" --post-webhook $URL` works in Azure Pipeline with only Bun installed
- [ ] Non-interactive mode fails with clear error if required flags (`--pipeline`, webhook) are missing
- [ ] All existing local/interactive workflows remain unaffected
- [ ] Typecheck + build pass for release package
