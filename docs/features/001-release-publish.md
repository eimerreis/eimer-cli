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
- **CI rename**: Pipeline script rewrites `package.json`:
  - `name`: `@scripts/release` -> `@tapio/release`
  - `version`: extracted from git tag (`release/v1.2.3` -> `1.2.3`)
  - Strips `dependencies`, `devDependencies` (everything is bundled)
  - Strips workspace-specific fields (`workspaces`, `scripts.postinstall`)
- **Publish trigger**: Git tag matching `release/v*`
- **Registry**: Azure Artifacts feed via `.npmrc` in CI

### Dual Build Support
- `build` (existing): `bunli build --native --outfile ../../bin/release` - personal native binary
- `build:publish` (new): `bun build src/index.ts --outdir dist --target bun` - npm-publishable bundle

## Implementation Plan
- [x] Inline config module - copy config loading into `src/config.ts`, own schema, path `~/.config/tapio-release/config.json`
- [x] Make area configs configurable - load from config file, merge with hardcoded defaults in `areas.ts`
- [x] Add `build:publish` script - Bun bundler targeting bun, bundled publish output in `dist/`
- [x] Create publish pipeline - Azure DevOps YAML triggered on `release/v*` tag: install -> build:publish -> rewrite package.json -> bun publish
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
- [ ] Git tag `release/v1.0.0` triggers CI and publishes to Azure Artifacts
- [x] README documents installation, usage, and config file format
