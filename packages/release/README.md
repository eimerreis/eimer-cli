# @scripts/release

CLI for Azure DevOps release workflows.

## Company distribution

This package is built from this repo and published in CI to Azure Artifacts as `@tapio/release`.

Install prerequisites:

```bash
curl -fsSL https://bun.sh/install | bash
az login
az devops configure -d organization=https://dev.azure.com/<org> project=<project>
```

Run directly without cloning:

```bash
bunx @tapio/release changelog --pipeline "MyPipeline"
bunx @tapio/release approve
```

## Configuration

Path: `~/.config/tapio-release/config.json`

```json
{
  "teams": {
    "webhookUrl": "https://..."
  },
  "release": {
    "defaultPipeline": "example-release-pipeline"
  },
  "areas": {
    "frontend": {
      "includeScopes": ["frontend", "ui", "web"],
      "excludeScopes": ["backend", "api", "infra"],
      "excludeKeywords": ["sql migration"]
    },
    "mobile": {
      "includeScopes": ["mobile", "ios", "android"],
      "excludeScopes": ["backend", "infra"],
      "excludeKeywords": []
    }
  }
}
```

Notes:
- `teams.webhookUrl` is used when `changelog` runs without `--post-webhook`
- `release.defaultPipeline` is used as the default pipeline prompt value
- `areas` entries merge with defaults (`frontend`, `backend`, `infra`) and can override them

## Development

```bash
bun run dev -- <command>
```

## Build

```bash
# Native binary for local personal use
bun run build

# Bundled output for registry publishing
bun run build:publish
```

Native build outputs global binary to `../../bin/release`.

## Commands

- `release changelog`: Build grouped changelog from pipeline run commit range and copy to clipboard
- `release approve`: Approve pending Azure DevOps pipeline approvals

### Post changelog to Teams webhook

```bash
release changelog --pipeline "example-release-pipeline" --post-webhook "https://..."
```

The command shows the changelog before posting, lets you edit it in your editor, and asks for confirmation before sending. If VS Code is available, you can also choose an edit flow where the next file save posts automatically. That flow writes a visible `eimer-release-changelog-*.md` file into the current working directory so you can inspect it directly. It tries an Adaptive Card payload first, then falls back to a MessageCard payload if needed.

## Publish pipeline

- GitHub Actions workflow: `.github/workflows/publish-release.yml`
- Trigger: pushes to `main` with pending `.changeset/*.md` files
- Create release intent: `bunx changeset`
- Package rewrite in CI: `@scripts/release` -> `@tapio/release`, version from Changesets
- Release outputs: version commit, `release/v<version>` tag, Azure Artifacts publish, GitHub release
- GitHub environment: `tapioone-azdevops` (`AZDEVOPS_ORGANIZATION`, `AZDEVOPS_PROJECT`, `AZDEVOPS_PACKAGEFEED`, plus publish token like `AZDEVOPS_PAT`)
- Registry auth template: `docs/npmrc.azure-artifacts.template`
