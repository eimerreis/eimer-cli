# @scripts/release

CLI for Azure DevOps release workflows.


## Configuration

Path: `~/.config/tapio-release/config.json`

```json
{
  "teams": {
    "webhookUrl": "https://...",
    "channels": {
      "frontend-releases": "https://...",
      "backend-releases": "https://..."
    }
  },
  "release": {
    "defaultPipeline": "example-release-pipeline",
    "prodStageName": "deploy_production"
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
- `teams.webhookUrl` is the default channel for backward compatibility
- `teams.channels` lets you configure named webhook targets (use with `--channel <name>`)
- `release.defaultPipeline` is used as the default pipeline prompt value
- `release.prodStageName` overrides automatic prod-stage detection in release run scanning
- `areas` entries merge with defaults (`frontend`, `backend`, `infra`) and can override them

## Azure DevOps auth/context resolution

Auth precedence:
1. `SYSTEM_ACCESSTOKEN` (Azure Pipelines OAuth token)
2. `AZURE_DEVOPS_PAT` (manual PAT, useful outside Azure Pipelines)
3. `az account get-access-token` fallback (local developer flow)

Context precedence:
1. `SYSTEM_COLLECTIONURI` + `SYSTEM_TEAMPROJECT`
2. `az devops configure --list` fallback (local developer flow)

This means CI does not need Azure CLI if pipeline variables are present.

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

## CI usage (Azure Pipelines)

In CI/non-interactive mode:
- pass `--pipeline <name>` or configure `release.defaultPipeline`
- pass `--post-webhook <url>` or `--channel <name>`
- optionally pass `--prod-stage-name <stage>` to avoid relying on config for prod stage detection
- clipboard copy and interactive review are skipped automatically

Example pipeline step (after successful prod deployment stage):

```yaml
- script: |
    bunx @tapio/release changelog \
      --pipeline "$(ReleasePipelineName)" \
      --prod-stage-name "$(ProdStageName)" \
      --post-webhook "$(TEAMS_RELEASE_WEBHOOK)" \
      --no-copy
  displayName: "Post release changelog"
  env:
    SYSTEM_ACCESSTOKEN: $(System.AccessToken)
    SYSTEM_COLLECTIONURI: $(System.CollectionUri)
    SYSTEM_TEAMPROJECT: $(System.TeamProject)
```

Important Azure Pipelines setting:
- enable **Allow scripts to access OAuth token** so `$(System.AccessToken)` is available

## Publish pipeline

- GitHub Actions workflow: `.github/workflows/publish-release.yml`
- Trigger: pushes to `main` with pending `.changeset/*.md` files
- Create release intent: `bunx changeset`
- Release outputs: version commit, `release/v<version>` tag, Azure Artifacts publish, GitHub release
- GitHub environment: `tapioone-azdevops` (`AZDEVOPS_ORGANIZATION`, `AZDEVOPS_PROJECT`, `AZDEVOPS_PACKAGEFEED`, plus publish token like `AZDEVOPS_PAT`)
- Registry auth template: `docs/npmrc.azure-artifacts.template`
