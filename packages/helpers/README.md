# @scripts/helpers

Ad-hoc helper scripts for local workflows.

## Environment

- `EIMER_TRANSLATION_PATHS`: path-delimited list of translation directories for i18n helper scripts
- `AZURE_DEVOPS_TEAM`: default Azure DevOps team for task helper scripts
- `AZURE_DEVOPS_AREA_PATH`: default Azure DevOps area path for task helper scripts
- `AZURE_DEVOPS_REVIEWER`: default reviewer for `prs-for-me.ts`
- `AZURE_DEVOPS_REQUESTED_FOR`: identity filter for `pipeline-runs.ts`

Examples:

```bash
export EIMER_TRANSLATION_PATHS="$HOME/Sources/app-a/translations:$HOME/Sources/app-b/translations"
export AZURE_DEVOPS_TEAM="My Team"
export AZURE_DEVOPS_AREA_PATH="Company\\Engineering"
```
