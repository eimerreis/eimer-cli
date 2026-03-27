import { defineCommand, option } from "@bunli/core";
import { z } from "zod";
import { getAreaConfig, listAreas, mergeAreaConfigs, type AreaConfig } from "../areas";
import { loadConfig } from "../config";
import { copyToClipboard } from "./clipboard";
import { buildMarkdownChangelog, countGroupedCommits, groupCommitsByType } from "./changelog-format";
import { reviewTeamsMarkdown } from "./teams-review";
import { postChangelogToTeams } from "./teams-webhook";
import {
  buildRunUrl,
  getAzureContext,
  normalizeText,
  resolvePipelineByName,
  runJson,
  type CommitInfo,
  type PipelineRun,
} from "./utils";

type AzureGitCommit = {
  commitId?: string;
  comment?: string;
};

type AzureGitCommitResponse = {
  count?: number;
  value?: AzureGitCommit[];
};

const changelogCommand = defineCommand({
  name: "changelog",
  description: "Build release changelog from remote master commits",
  options: {
    pipeline: option(z.string().trim().optional(), {
      short: "p",
      description: "Pipeline name",
    }),
    from: option(z.coerce.number().int().positive().optional(), {
      short: "f",
      description: "From run ID override (defaults to latest prod-success run on master)",
    }),
    to: option(z.string().trim().optional(), {
      short: "t",
      description: "To commit SHA override (default: latest successful master pipeline run)",
    }),
    area: option(z.string().trim().optional(), {
      short: "a",
      description: "Area filter from predefined configs",
    }),
    "no-copy": option(z.coerce.boolean().default(false), {
      description: "Print only, do not copy changelog to clipboard",
    }),
    "post-webhook": option(z.string().trim().optional(), {
      description: "Post changelog to Microsoft Teams incoming webhook URL",
    }),
    json: option(z.coerce.boolean().default(false), {
      short: "j",
      description: "Print machine-readable JSON",
    }),
  },
  handler: async ({ flags, prompt }) => {
    try {
      const config = await loadConfig();
      const context = await getAzureContext();
      const prodStageCache = new Map<number, boolean>();
      const defaultPipeline = (config.release?.defaultPipeline || "").trim();
      const mergedAreaConfigs = mergeAreaConfigs(config.areas);

      let pipelineName = (flags.pipeline || "").trim();
      if (!pipelineName) {
        pipelineName = (
          await prompt.text("Pipeline name", {
            placeholder: defaultPipeline || "example-release-pipeline",
            fallbackValue: defaultPipeline,
            validate: (value) => (value.trim().length > 0 ? true : "Pipeline name is required"),
          })
        ).trim();
      }

      if (!pipelineName) {
        throw new Error("Missing pipeline name. Pass --pipeline.");
      }

      const pipeline = await resolvePipelineByName(pipelineName);
      let fromRun = await resolveFromRun(pipeline.id, context.baseUrl, flags.from, prodStageCache);
      let fromCommit = (fromRun.sourceVersion || "").trim();
      let targetRun: PipelineRun | null = null;

      if (!fromCommit) {
        throw new Error(`Selected from run #${fromRun.id} has no sourceVersion.`);
      }

      const repositoryId = (fromRun.repository?.id || "").trim();
      if (!repositoryId) {
        throw new Error(`Selected from run #${fromRun.id} has no repository ID.`);
      }

      if (!flags.to) {
        targetRun = await resolveLatestSucceededMasterRun(pipeline.id);
      }

      const toCommit = flags.to ? normalizeCommitSha(flags.to) : (targetRun?.sourceVersion || "").trim();

      let effectiveToCommit = toCommit;
      let toRunId = flags.to ? undefined : targetRun?.id;

      if (!effectiveToCommit) {
        throw new Error("Could not resolve target commit from latest successful master pipeline run.");
      }

      if (fromCommit.toLowerCase() === effectiveToCommit.toLowerCase()) {
        const currentReleaseRun = targetRun || fromRun;
        const previousRun = await resolvePreviousProdReleaseRun(
          pipeline.id,
          context.baseUrl,
          currentReleaseRun.id,
          prodStageCache,
        );
        if (!previousRun) {
          throw new Error("No new master commits and no previous successful prod release run found for fallback.");
        }

        const previousCommit = (previousRun.sourceVersion || "").trim();
        if (!previousCommit) {
          throw new Error(`Fallback run #${previousRun.id} has no sourceVersion.`);
        }

        const currentReleaseCommit = (currentReleaseRun.sourceVersion || "").trim();
        if (!currentReleaseCommit) {
          throw new Error(`Current release run #${currentReleaseRun.id} has no sourceVersion.`);
        }

        toRunId = currentReleaseRun.id;
        fromRun = previousRun;
        fromCommit = previousCommit;
        effectiveToCommit = currentReleaseCommit;
      }

      const commits = await loadRemoteMasterCommitRange({
        baseUrl: context.baseUrl,
        repositoryId,
        fromCommit,
        toCommit: effectiveToCommit,
      });

      const { included, manualReview, areaName } = filterCommitsByArea(commits, flags.area, mergedAreaConfigs);
      const grouped = groupCommitsByType(included);

      let markdown = buildMarkdownChangelog({
        pipelineName: pipeline.name || pipelineName,
        fromRunId: fromRun.id,
        toRunId,
        fromCommit,
        toCommit: effectiveToCommit,
        area: areaName,
        included: grouped,
        manualReview,
      });

      const webhookUrl = (flags["post-webhook"] || config.teams?.webhookUrl || "").trim();
      let skippedWebhookPost = false;

      if (webhookUrl && !flags.json && process.stdin.isTTY && process.stdout.isTTY) {
        const review = await reviewTeamsMarkdown(markdown, prompt as unknown as {
          select(
            message: string,
            options: {
              options: Array<{
                value: "post" | "edit" | "cancel";
                label: string;
                hint?: string;
              }>;
              default?: "post" | "edit" | "cancel";
            },
          ): Promise<"post" | "edit" | "cancel">;
        });

        markdown = review.markdown;
        skippedWebhookPost = !review.shouldPost;
      }

      let postedFormat: "adaptive-card" | "message-card" | null = null;
      if (webhookUrl && !skippedWebhookPost) {
        postedFormat = await postChangelogToTeams({
          webhookUrl,
          pipelineName: pipeline.name || pipelineName,
          fromRunId: fromRun.id,
          toRunId,
          fromCommit,
          toCommit: effectiveToCommit,
          area: areaName,
          pipelineRunUrl: toRunId ? buildRunUrl(context, toRunId) : undefined,
          markdown,
        });
      }

      if (flags.json) {
        console.log(
          JSON.stringify(
            {
              pipeline: {
                id: pipeline.id,
                name: pipeline.name || pipelineName,
              },
              range: {
                fromRunId: fromRun.id,
                toRunId: toRunId || null,
                fromCommit,
                toCommit: effectiveToCommit,
                branch: "master",
              },
              area: areaName || null,
              totalCommits: commits.length,
              includedCount: countGroupedCommits(grouped),
              manualReviewCount: manualReview.length,
              postedToWebhook: Boolean(webhookUrl) && !skippedWebhookPost && Boolean(postedFormat),
              postedFormat,
              grouped,
              manualReview,
              markdown,
            },
            null,
            2,
          ),
        );
        return;
      }

      console.log(markdown);

      if (!flags["no-copy"]) {
        await copyToClipboard(markdown);
        console.log("\nCopied changelog to clipboard.");
      }

      if (postedFormat) {
        console.log(`Posted changelog to Teams webhook (${postedFormat}).`);
      } else if (skippedWebhookPost) {
        console.log("Skipped Teams webhook post.");
      }
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      console.error(`Failed to build changelog: ${message}`);
      process.exit(1);
    }
  },
});

type BuildTimelineRecord = {
  type?: string;
  identifier?: string;
  name?: string;
  result?: string;
  state?: string;
};

type BuildTimeline = {
  records?: BuildTimelineRecord[];
};

async function resolveFromRun(
  pipelineId: number,
  baseUrl: string,
  fromRunId: number | undefined,
  prodStageCache: Map<number, boolean>,
): Promise<PipelineRun> {
  if (fromRunId) {
    const run = await runJson<PipelineRun>([
      "az",
      "pipelines",
      "runs",
      "show",
      "--id",
      String(fromRunId),
      "--detect",
      "true",
      "--output",
      "json",
    ]);
    validateRunIsMasterSuccess(run, pipelineId);
    return run;
  }

  const latestProdReleaseRun = await resolveLatestProdReleaseRun(pipelineId, baseUrl, prodStageCache);
  if (!latestProdReleaseRun) {
    throw new Error("No successful prod release run found on master. Pass --from <runId>.");
  }

  return latestProdReleaseRun;
}

async function resolveLatestProdReleaseRun(
  pipelineId: number,
  baseUrl: string,
  prodStageCache: Map<number, boolean>,
): Promise<PipelineRun | null> {
  const runs = await loadSucceededMasterRuns(pipelineId, 1);
  if (runs.length === 1) {
    const isProdSuccess = await hasSuccessfulProdStage(baseUrl, runs[0].id, prodStageCache);
    if (isProdSuccess) {
      return runs[0];
    }
  }

  const expandedRuns = await loadSucceededMasterRuns(pipelineId, 200);
  for (const run of expandedRuns) {
    if (await hasSuccessfulProdStage(baseUrl, run.id, prodStageCache)) {
      return run;
    }
  }

  return null;
}

async function resolveLatestSucceededMasterRun(pipelineId: number): Promise<PipelineRun | null> {
  const runs = await loadSucceededMasterRuns(pipelineId, 1);
  return runs[0] || null;
}

async function resolvePreviousProdReleaseRun(
  pipelineId: number,
  baseUrl: string,
  currentRunId: number,
  prodStageCache: Map<number, boolean>,
): Promise<PipelineRun | null> {
  const runs = await loadSucceededMasterRuns(pipelineId, 200);
  let passedCurrent = false;

  for (const run of runs) {
    const isProdSuccess = await hasSuccessfulProdStage(baseUrl, run.id, prodStageCache);
    if (!isProdSuccess) {
      continue;
    }

    if (!passedCurrent) {
      if (run.id === currentRunId) {
        passedCurrent = true;
      }

      continue;
    }

    return run;
  }

  return null;
}

async function hasSuccessfulProdStage(baseUrl: string, runId: number, cache: Map<number, boolean>): Promise<boolean> {
  const cached = cache.get(runId);
  if (typeof cached === "boolean") {
    return cached;
  }

  const timeline = await runJson<BuildTimeline>([
    "az",
    "rest",
    "--resource",
    "499b84ac-1321-427f-aa17-267ca6975798",
    "--method",
    "get",
    "--url",
    `${baseUrl}/_apis/build/builds/${runId}/timeline?api-version=7.1`,
    "--output",
    "json",
  ]);

  const records = timeline.records || [];
  const hasProd = records.some((record) => {
    if ((record.type || "").toLowerCase() !== "stage") {
      return false;
    }

    const identifier = (record.identifier || "").toLowerCase();
    const name = (record.name || "").toLowerCase();
    const identifierParts = identifier.split(/[^a-z0-9]+/).filter(Boolean);
    const nameParts = name.split(/[^a-z0-9]+/).filter(Boolean);
    const mentionsProd = identifier.includes("prod") || name.includes("prod");
    const hasStandaloneProdStage = identifierParts.includes("prod") || nameParts.includes("prod");
    const hasDeployProdStage =
      (identifierParts.includes("deploy") && identifierParts.includes("prod")) ||
      (nameParts.includes("deploy") && nameParts.includes("prod"));
    const isProdStage =
      identifier === "deploy_prod" ||
      hasDeployProdStage ||
      hasStandaloneProdStage ||
      (mentionsProd && (identifier.includes("production") || name.includes("production")));

    if (!isProdStage) {
      return false;
    }

    return (record.state || "").toLowerCase() === "completed" && (record.result || "").toLowerCase() === "succeeded";
  });

  cache.set(runId, hasProd);
  return hasProd;
}

async function loadSucceededMasterRuns(pipelineId: number, top: number): Promise<PipelineRun[]> {
  return runJson<PipelineRun[]>([
    "az",
    "pipelines",
    "runs",
    "list",
    "--pipeline-ids",
    String(pipelineId),
    "--status",
    "completed",
    "--result",
    "succeeded",
    "--branch",
    "master",
    "--top",
    String(top),
    "--query-order",
    "QueueTimeDesc",
    "--detect",
    "true",
    "--output",
    "json",
  ]);
}

function validateRunIsMasterSuccess(run: PipelineRun, pipelineId: number): void {
  if (run.definition?.id && run.definition.id !== pipelineId) {
    throw new Error(`Run #${run.id} does not belong to selected pipeline ID ${pipelineId}.`);
  }

  if ((run.status || "").toLowerCase() !== "completed") {
    throw new Error(`Run #${run.id} is not completed.`);
  }

  if ((run.result || "").toLowerCase() !== "succeeded") {
    throw new Error(`Run #${run.id} is not succeeded.`);
  }

  const branch = normalizeBranch(run.sourceBranch || "");
  if (branch !== "master") {
    throw new Error(`Run #${run.id} is on branch '${run.sourceBranch || ""}', expected master.`);
  }
}

async function resolveLatestMasterCommit(baseUrl: string, repositoryId: string): Promise<string> {
  const response = await runJson<AzureGitCommitResponse>([
    "az",
    "rest",
    "--resource",
    "499b84ac-1321-427f-aa17-267ca6975798",
    "--method",
    "get",
    "--url",
    `${baseUrl}/_apis/git/repositories/${repositoryId}/commits?searchCriteria.itemVersion.version=master&searchCriteria.itemVersion.versionType=branch&searchCriteria.historyMode=firstParent&searchCriteria.$top=1&api-version=7.1`,
    "--output",
    "json",
  ]);

  return (response.value?.[0]?.commitId || "").trim();
}

async function loadRemoteMasterCommitRange(options: {
  baseUrl: string;
  repositoryId: string;
  fromCommit: string;
  toCommit: string;
}): Promise<CommitInfo[]> {
  const commits: CommitInfo[] = [];
  const seen = new Set<string>();
  let skip = 0;
  const pageSize = 100;
  const maxPages = 20;

  for (let page = 0; page < maxPages; page += 1) {
    const response = await runJson<AzureGitCommitResponse>([
      "az",
      "rest",
      "--resource",
      "499b84ac-1321-427f-aa17-267ca6975798",
      "--method",
      "get",
      "--url",
      `${options.baseUrl}/_apis/git/repositories/${options.repositoryId}/commits?searchCriteria.itemVersion.version=${options.toCommit}&searchCriteria.itemVersion.versionType=commit&searchCriteria.historyMode=firstParent&searchCriteria.$top=${pageSize}&searchCriteria.$skip=${skip}&api-version=7.1`,
      "--output",
      "json",
    ]);

    const items = response.value || [];
    if (items.length === 0) {
      break;
    }

    let reachedFrom = false;
    for (const item of items) {
      const hash = (item.commitId || "").trim();
      if (!hash || seen.has(hash)) {
        continue;
      }
      seen.add(hash);

      if (hash.toLowerCase() === options.fromCommit.toLowerCase()) {
        reachedFrom = true;
        break;
      }

      const subject = (item.comment || "").split(/\r?\n/, 1)[0]?.trim() || hash;
      const parsed = parseSubjectAndPr(subject, options.baseUrl, options.repositoryId);
      commits.push({
        hash,
        shortHash: hash.slice(0, 7),
        subject: parsed.subject,
        conventional: parseConventionalCommit(parsed.subject),
        prNumber: parsed.prNumber,
        prUrl: parsed.prUrl,
      });
    }

    if (reachedFrom) {
      return commits;
    }

    skip += items.length;
  }

  throw new Error(
    `Could not resolve master commit range from ${options.fromCommit.slice(0, 7)} to ${options.toCommit.slice(0, 7)} within first-parent history.`,
  );
}

function parseSubjectAndPr(
  subject: string,
  baseUrl: string,
  repositoryId: string,
): { subject: string; prNumber?: number; prUrl?: string } {
  const mergeMatch = subject.match(/^Merged PR\s+(\d+):\s*(.+)$/i);
  if (!mergeMatch) {
    return { subject };
  }

  const prNumber = Number.parseInt(mergeMatch[1], 10);
  const trimmedSubject = (mergeMatch[2] || "").trim() || subject;
  if (!Number.isFinite(prNumber) || prNumber <= 0) {
    return { subject: trimmedSubject };
  }

  return {
    subject: trimmedSubject,
    prNumber,
    prUrl: `${baseUrl}/_git/${repositoryId}/pullrequest/${prNumber}`,
  };
}

function parseConventionalCommit(subject: string): CommitInfo["conventional"] {
  const match = subject.match(/^([a-z]+)(?:\(([^)]+)\))?(!)?:\s*(.+)$/i);
  if (!match) {
    return {
      type: "",
      scope: "",
      description: subject,
      raw: subject,
      isConventional: false,
    };
  }

  return {
    type: (match[1] || "").toLowerCase(),
    scope: (match[2] || "").toLowerCase(),
    description: (match[4] || "").trim(),
    raw: subject,
    isConventional: true,
  };
}

function normalizeCommitSha(value: string): string {
  const trimmed = value.trim().toLowerCase();
  if (!/^[0-9a-f]{7,40}$/i.test(trimmed)) {
    throw new Error(`Invalid commit SHA '${value}'.`);
  }

  return trimmed;
}

function normalizeBranch(value: string): string {
  return value.replace(/^refs\/heads\//i, "").trim().toLowerCase();
}

function filterCommitsByArea(
  commits: CommitInfo[],
  areaName?: string,
  areaConfigs?: Record<string, AreaConfig>,
): {
  included: CommitInfo[];
  manualReview: CommitInfo[];
  areaName: string;
} {
  const normalizedArea = (areaName || "").trim().toLowerCase();
  if (!normalizedArea) {
    return {
      included: commits,
      manualReview: [],
      areaName: "",
    };
  }

  const config = getAreaConfig(normalizedArea, areaConfigs);
  if (!config) {
    throw new Error(`Unknown area '${areaName}'. Available areas: ${listAreas(areaConfigs).join(", ")}`);
  }

  const includeScopes = new Set(config.includeScopes.map(normalizeText));
  const excludeScopes = new Set(config.excludeScopes.map(normalizeText));
  const excludeKeywords = config.excludeKeywords.map(normalizeText);

  const included: CommitInfo[] = [];
  const manualReview: CommitInfo[] = [];

  for (const commit of commits) {
    const subject = normalizeText(commit.subject);
    const scope = normalizeText(commit.conventional.scope || "");

    if (excludeKeywords.some((keyword) => keyword && subject.includes(keyword))) {
      continue;
    }

    if (scope && excludeScopes.has(scope)) {
      continue;
    }

    if (scope && includeScopes.has(scope)) {
      included.push(commit);
      continue;
    }

    manualReview.push(commit);
  }

  return {
    included,
    manualReview,
    areaName: normalizedArea,
  };
}

export default changelogCommand;
