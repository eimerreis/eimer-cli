## Changesets

Use Changesets to describe release-worthy changes for `@scripts/release`.

Create a changeset before merging to `main`:

```bash
bunx changeset
```

Select `@scripts/release`, choose the bump type, and write a short summary.

When that changeset reaches `main`, the release workflow will:

- version `@scripts/release`
- publish `@tapio/release` to Azure Artifacts
- create a git tag and GitHub release
