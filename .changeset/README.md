## Changesets

Every pull request adds a changeset; see "Releases and changesets" in [AGENTS.md](../AGENTS.md).

```bash
bunx changeset          # choose eimer and/or @scripts/release, the bump and a summary
bunx changeset --empty  # nothing ships
```

Write the summary for someone reading the changelog. On `main`, the Release workflow consumes the changesets, writes the changelogs and tags each release unit.
