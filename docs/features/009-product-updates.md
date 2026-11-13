# Feature: Product Updates (`eimer product`)

## Overview
Products that release with Changesets already have accurate, versioned changelogs, but nothing turns them into public update posts (a Framer-style "Updates" page). This feature adds `eimer product`, which finds user-facing changes that no post covers yet, lets me pick the ones that belong to one feature, writes a draft update file into the repo and copies a prompt for a coding agent. The agent writes the copy, captures screenshots and opens a PR.

The CLI never calls a model. It handles everything deterministic (parsing, filtering, state, validation) and leaves wording and media to the agent. Because the handoff is a file in the repo plus a prompt, a cloud agent can later run the same step unattended.

First consumer: `volleyball-team-app` (website `marketing/website`, products `volleyteam-app`, `volleyteam-backend`, `volleyteam-website-release`).

## User Stories
- As a solo maintainer, I want `eimer product update` to show me what changed for users since the last update post, so I never dig through changelogs by hand.
- As a maintainer, I want internal changes (CI, release plumbing, refactors) filtered out by default and remembered once I dismiss them.
- As a maintainer, I want one command to hand my agent everything it needs to write a post with real screenshots, so publishing an update takes minutes.
- As a maintainer, I want the website's CI to reject broken update files (missing image, missing language).

## Decisions
- **One post per run = one feature.** Like Framer, posts are about a feature, not a version; several posts can share a date. Fixes join the next feature post under "Also in this release", or a periodic "Fixes and improvements" post.
- **Filter by commit type only.** No new changeset convention; the interactive pick is the real filter. Revisit (e.g. a `user-facing:` line in the changeset body) once updates run unattended.
- **State lives in the update files.** A post's `changes` list holds the change ids (commit hashes) it covers; no separate cursor. Ids, not versions, because one version can hold several features. Dismissed entries go to an ignore file. `since` hides history from before the tool was adopted.
- **Prompt template lives in the repo**, with a built-in default in the CLI.
- **Media default is a real capture.** Stageshow (faux animated UI) only for headline features where motion is the point; app-store-screenshots only when the store listing should be refreshed too.

## Commands
```
eimer product update     # pick changes → draft file → prompt to clipboard
eimer product status     # count of uncovered user-facing entries (--json)
eimer product check      # validate update files; exit 1 on errors (CI)
eimer product init       # write .eimer/product.json with detected products
```

## Repo config (`.eimer/product.json`)
```json
{
  "since": "2026-11-13",
  "products": {
    "volleyteam-app": { "changelog": "packages/release-app/CHANGELOG.md", "include": true },
    "volleyteam-website-release": { "changelog": "packages/release-website/CHANGELOG.md", "include": true },
    "volleyteam-backend": { "changelog": "packages/release-backend/CHANGELOG.md", "include": false }
  },
  "platforms": {
    "iphone": ["apps/ios/Sources/**", "!apps/ios/Sources/**/Mac*/**"],
    "mac": ["apps/ios/Sources/**/Mac*/**"],
    "android": ["apps/android/**"],
    "web": ["marketing/website/**"]
  },
  "internalTypes": ["ci", "chore", "build", "test", "refactor", "docs", "style"],
  "updatesDir": "marketing/website/content/updates",
  "mediaDir": "marketing/website/public/updates",
  "mediaUrl": "/updates",
  "languages": ["en", "de"],
  "capture": {
    "iphone": "cd apps/ios && fastlane capture only_testing:{test}",
    "mac": "cd apps/ios && fastlane capture_mac only_testing:{test}",
    "android": "cd apps/android && ./gradlew recordRoborazziDebug --tests {test}"
  },
  "promptTemplate": ".eimer/product-update.prompt.md"
}
```
`include: false` products start unchecked (backend changes rarely matter to users on their own). Capture commands are examples; the repo defines its own. `ignoreFile` defaults to `<updatesDir>/.ignored.json`; `promptTemplate` falls back to the built-in prompt.

## `product update` flow
1. **Collect.** Parse each product's `CHANGELOG.md` (Changesets format: `## <version>` → `### Major|Minor|Patch Changes` → `- <hash>: <text>`; dependency-only bumps skipped). Drop change ids listed in any post's `changes` (drafts included), ids in the ignore file and commits older than `since`. Entries without a hash get a text-based id (`t` + 7 hex).
2. **Enrich.** For each entry's commit hash: conventional-commit type and scope from the subject, changed paths from `git show --name-only`. Paths map to platform tags via `platforms` globs. Entries sharing a hash across products (one changeset bumping several products) are merged into one row.
3. **Pre-filter.** Entries whose type is in `internalTypes`, or that belong only to `include: false` products, start unchecked and dimmed.
4. **Pick.** Multi-select grouped by version (newest first), each row showing type, platforms and the first line of text. Then:
   - Title hint (optional free text).
   - Media: `auto` (default) / `screenshot` / `animated demo` / `none`.
   - Also refresh store screenshots? (y/N)
   - Hide the unchecked internal entries for good? (Y/n) → appended to the ignore file. Unchecked user-facing entries stay pending for the next post.
5. **Draft.** Write `<updatesDir>/<date>-<slug>.<lang>.md` for the primary language with `status: draft`, front matter filled from the selection and the raw changelog text as body. The slug comes from the title hint or the first selected entry.
6. **Prompt.** Render the template (the repo's file, or the built-in default) with the draft path, selected entries, platforms, media choice, capture commands and languages. Copy to clipboard (reuse `core::process::copy_to_clipboard`); `--print` writes to stdout, `--out <file>` to a file. `--json` returns the selection and draft path for automation.

Non-interactive: `--select <id,...>` (unique prefixes ok, `all` = every user-facing change), `--title`, `--media`, `--store`, `--ignore-internal`, `--date` allow scripting and future cloud runs; `--select` skips the picker and every follow-up question. The default title is the first sentence of the first change, cut to 48 characters; the agent may rename the post files when it writes the real title.

## Update file format
```md
---
title: VolleyTeam for Mac
date: 2026-11-12
platforms: [mac]
releases: [volleyteam-app@1.38.0, volleyteam-app@1.38.1]
changes: [cd4a817, ef71e4e]
media:
  kind: image            # image | video | demo | none
  src: /updates/volleyteam-for-mac/planner-light.png
  dark: /updates/volleyteam-for-mac/planner-dark.png
  alt: Training planner on Mac with two blocks and the drill library
status: published        # draft | published
---
Plan your training on a big screen. …

## Also in this release
- Drag players between court and bench
```
One file per language (`<slug>.en.md`, `<slug>.de.md`) sharing the front matter except text fields. The front matter is a strict YAML subset (scalars, inline lists, one nested level) so any static-site tool can read it. The website renders only `published` files. The filters on the page (All / iPhone / Mac / Web) read `platforms`.

## Prompt contract (what the template must tell the agent)
- Read the draft and the listed commits; rewrite in user language, in the site's voice (`DESIGN.md`/`PRODUCT.md`), in every configured language. No internal names, no claims beyond the changes.
- **Capture:** find a UI test that covers the feature and run its capture command for light and dark. If none exists, add a focused UI test with the sample team, then capture. Never ship real user data.
- **Media rules:** real capture by default; Stageshow only if media is `animated demo`, or `auto` and the feature is a flow where motion is the point; app-store-screenshots only if the store refresh was requested, in its own folder, not part of the post.
- Save media to `<mediaDir>/<slug>/`, set `status: published`, run `eimer product check` and the website build, add a Website changeset, open a PR.

## `product check`
Validates every update file: file name, configured language, front matter (title, date matching the file name, known platforms, `changes` ids present in the changelogs, status), and for published posts the media block (`kind`; `src`/`dark` under `mediaUrl` with existing files and `alt` for image/video; an existing `component` for demo). Every published post must exist in every language. Also checks the repo prompt template for unknown placeholders. Drafts are warnings (`--strict`: errors). Exit 1 on any error; `--json` for tooling. The website's CI runs it.

## Implementation Plan
- [x] Config: `.eimer/product.json` schema + loader (walk up from cwd), `product init` with product detection from `.changeset/config.json` and package changelogs.
- [x] Changelog parser for the Changesets format, with fixture tests (multi-paragraph entries, entries without hash).
- [x] Git enrichment: commit type/scope, changed paths, glob → platform mapping; merge entries by hash.
- [x] Coverage state: read `changes` ids from update files, ignore file read/append.
- [x] `product status` and `product check` (pure, testable on fixture dirs).
- [x] `product update`: dialoguer multi-select + questions, draft writer, prompt rendering (built-in default template + repo override), clipboard / `--print` / `--out` / `--json`, non-interactive flags.
- [x] Docs: README command table, `docs/product.md`.
- [ ] Consumer setup in volleyball-team-app (separate PR): config, prompt template, update page reading `content/updates`, `product check` in website CI, capture lanes for iOS/Mac.

## Later
- `eimer product update --cloud`: start a scheduled or on-release cloud agent with the rendered prompt (needs the `user-facing` marker to replace the interactive pick).
- RSS feed generated from the update files.
