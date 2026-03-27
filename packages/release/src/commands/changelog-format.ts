import type { ChangelogGroups, CommitInfo } from "./utils";

function createEmptyGroups(): ChangelogGroups {
  return {
    features: [],
    fixes: [],
    chores: [],
    docs: [],
    refactors: [],
    perf: [],
    tests: [],
    buildCi: [],
    other: [],
  };
}

function groupCommitsByType(commits: CommitInfo[]): ChangelogGroups {
  const groups = createEmptyGroups();
  for (const commit of commits) {
    const type = commit.conventional.type;

    if (type === "feat") {
      groups.features.push(commit);
      continue;
    }

    if (type === "fix") {
      groups.fixes.push(commit);
      continue;
    }

    if (type === "chore") {
      groups.chores.push(commit);
      continue;
    }

    if (type === "docs") {
      groups.docs.push(commit);
      continue;
    }

    if (type === "refactor") {
      groups.refactors.push(commit);
      continue;
    }

    if (type === "perf") {
      groups.perf.push(commit);
      continue;
    }

    if (type === "test") {
      groups.tests.push(commit);
      continue;
    }

    if (type === "build" || type === "ci") {
      groups.buildCi.push(commit);
      continue;
    }

    groups.other.push(commit);
  }

  return groups;
}

function countGroupedCommits(groups: ChangelogGroups): number {
  return (
    groups.features.length +
    groups.fixes.length +
    groups.chores.length +
    groups.docs.length +
    groups.refactors.length +
    groups.perf.length +
    groups.tests.length +
    groups.buildCi.length +
    groups.other.length
  );
}

function formatCommitBullet(commit: CommitInfo): string {
  const prSuffix = commit.prNumber
    ? commit.prUrl
      ? ` ([PR ${commit.prNumber}](${commit.prUrl}))`
      : ` (PR ${commit.prNumber})`
    : "";

  const hashSuffix = commit.prUrl ? "" : ` (\`${commit.shortHash}\`)`;

  return `- ${commit.subject}${prSuffix}${hashSuffix}`;
}

function appendGroup(lines: string[], title: string, commits: CommitInfo[]): void {
  if (commits.length === 0) {
    return;
  }

  lines.push(`### ${title}`);
  for (const commit of commits) {
    lines.push(formatCommitBullet(commit));
  }
  lines.push("");
}

function buildMarkdownChangelog(params: {
  pipelineName: string;
  fromRunId?: number;
  toRunId?: number;
  fromCommit: string;
  toCommit: string;
  area?: string;
  included: ChangelogGroups;
  manualReview: CommitInfo[];
}): string {
  const areaSuffix = params.area ? ` | area: ${params.area}` : "";
  const fromLabel = params.fromRunId ? `run #${params.fromRunId}` : params.fromCommit.slice(0, 7);
  const toLabel = params.toRunId ? `run #${params.toRunId}` : params.toCommit.slice(0, 7);
  const lines = [
    `## Release Changelog`,
    ``,
    `Pipeline: ${params.pipelineName}`,
    `Range: ${fromLabel} -> ${toLabel}${areaSuffix}`,
    `Commits: ${params.fromCommit.slice(0, 7)}..${params.toCommit.slice(0, 7)}`,
    ``,
  ];

  appendGroup(lines, "Features", params.included.features);
  appendGroup(lines, "Fixes", params.included.fixes);
  appendGroup(lines, "Chores", params.included.chores);
  appendGroup(lines, "Docs", params.included.docs);
  appendGroup(lines, "Refactors", params.included.refactors);
  appendGroup(lines, "Perf", params.included.perf);
  appendGroup(lines, "Tests", params.included.tests);
  appendGroup(lines, "Build/CI", params.included.buildCi);
  appendGroup(lines, "Other", params.included.other);

  if (params.manualReview.length > 0) {
    lines.push(`### Needs Manual Review`);
    for (const commit of params.manualReview) {
      lines.push(formatCommitBullet(commit));
    }
    lines.push("");
  }

  if (lines[lines.length - 1] === "") {
    lines.pop();
  }

  return lines.join("\n");
}

export { buildMarkdownChangelog, countGroupedCommits, groupCommitsByType };
