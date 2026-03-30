# Feature: Publish Release CLI to Azure Artifacts

## Overview
Make `@scripts/release` deployable company-wide as `@tapio/release` via Azure Artifacts. Source stays in this monorepo; a CI pipeline renames the package and publishes on tag. Users run `bunx @tapio/release changelog --pipeline "..."` (requires Bun).

## User Stories
- As a release manager, I want to run `bunx @tapio/release changelog --pipeline "MyApp"` to generate a changelog without cloning this repo
- As a developer, I want to run `bunx @tapio/release approve` to approve pending deployments from my terminal
- As a CI pipeline, I want to run `bunx @tapio/release changelog --pipeline "MyApp" --json --post-webhook $URL` to auto-post changelogs on prod deploy

## Technical Approach

### No Framework Migration
Keep Bunli and all Bun APIs. Users must have Bun installed (`curl -fsSL https://bun.sh/install | bash`). Bunli's core uses `Bun.$` and `Bun.stringWidth` internally, making Node compatibility impractical without polyfills.

### Config Inlining
Copy config loading logic (~30 LOC) from `@scripts/config` into the release package. Change config path from `~/.config/eimer/config.json` to `~/.config/tapio-release/config.json`. Remove the `@scripts/config` workspace dependency.

### Extended Config Schema
```typescript
{
  teams?: { webhookUrl?: string },
  release?: { defaultPipeline?: string },
  areas?: {
    [name: string]: {
      includeScopes: string[],
      excludeScopes: string[],
      excludeKeywords: string[]
    }
  }
}
```

Hardcoded area configs (frontend/backend/infra) remain as defaults. User-defined areas in config merge with and can override defaults.

### Build & Publish Strategy
- **Build**: `bun build src/index.ts --outdir dist --target bun` - bundled entry point plus required runtime assets in `dist/`
- **Bin entry**: `dist/index.js` with `#!/usr/bin/env bun` shebang
- **CI rename**: Workflow script rewrites `package.json`:
  - `name`: `@scripts/release` -> `@tapio/release`
  - `version`: taken from Changesets-updated `packages/release/package.json`
  - Strips `dependencies`, `devDependencies` (everything is bundled)
  - Strips workspace-specific fields (`workspaces`, `scripts.postinstall`)
- **Publish trigger**: Push to `main` with pending `.changeset/*.md` files
- **Versioning**: `bunx changeset version` updates package version + changelog, removes processed changesets, and release workflow commits the result
- **Registry**: Azure Artifacts feed via `.npmrc` in GitHub Actions
- **Registry config**: GitHub environment `tapioone-azdevops` secrets `AZDEVOPS_ORGANIZATION`, `AZDEVOPS_PROJECT`, `AZDEVOPS_PACKAGEFEED`, and a publish token (`AZDEVOPS_PAT` preferred)
- **GitHub release**: Created from the matching `packages/release/CHANGELOG.md` entry and tagged as `release/v<version>`

### Dual Build Support
- `build` (existing): `bunli build --native --outfile ../../bin/release` - personal native binary
- `build:publish` (new): `bun build src/index.ts --outdir dist --target bun` - npm-publishable bundle

## Implementation Plan
- [x] Inline config module - copy config loading into `src/config.ts`, own schema, path `~/.config/tapio-release/config.json`
- [x] Make area configs configurable - load from config file, merge with hardcoded defaults in `areas.ts`
- [x] Add `build:publish` script - Bun bundler targeting bun, bundled publish output in `dist/`
- [x] Add Changesets CLI and config - release intent files under `.changeset/` and automated versioning on `main`
- [x] Create publish pipeline - GitHub Actions workflow on `main`: changeset version -> commit + tag -> build:publish -> rewrite package.json -> bun publish -> GitHub release
- [x] Add `.npmrc` template - document Azure Artifacts feed setup for consumers
- [x] Write README - installation (Bun + az CLI), usage examples, config reference, CI integration
- [ ] Test bunx flow end-to-end - verify from clean environment with only Bun + az CLI (blocked: requires Azure Artifacts feed credentials and published package)

## Dependencies
- Bun >= 1.0 on user machine
- `az` CLI installed and configured (`az login` + `az devops configure`)
- Azure Artifacts feed with `@tapio` scope configured

## Acceptance Criteria
- [ ] `bunx @tapio/release changelog --pipeline "SomePipeline"` works with only Bun + az CLI
- [ ] `bunx @tapio/release approve` lists and approves pending deployments
- [ ] `--json` flag works for all commands
- [ ] Teams webhook posting works via `--post-webhook` flag or config file
- [ ] Custom area definitions in `~/.config/tapio-release/config.json` are respected
- [x] Existing `bunli build --native` still produces personal native binary
- [ ] Merging a changeset to `main` versions `@scripts/release`, publishes `@tapio/release`, and creates a GitHub release
- [x] README documents installation, usage, and config file format
