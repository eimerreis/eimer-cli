import { defineCommand, option } from "@bunli/core";
import { printError, printInfo, printSuccess, withSpinner } from "@scripts/ui";
import { z } from "zod";
import { getAreaConfig, listAreas, mergeAreaConfigs, type AreaConfig } from "../areas";
import { loadConfig, resolveChannels } from "../config";
import { copyToClipboard } from "./clipboard";
import { buildMarkdownChangelog, countGroupedCommits, groupCommitsByType } from "./changelog-format";
import { hasSuccessfulProdStage, type ProdStageCache } from "./prod-stage";
import { reviewTeamsMarkdown } from "./teams-review";
import { postChangelogToTeams } from "./teams-webhook";
import {
  buildRunUrl,
  getAzureClient,
  getAzureContext,
  loadPipelineRuns,
  loadRunById,
  normalizeText,
  resolveStringArg,
  resolvePipelineByName,
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
  description: "Build release changelog from pipeline commits on a branch",
  options: {
    pipeline: option(z.string().trim().optional(), {
      short: "p",
      description: "Pipeline name",
    }),
    branch: option(z.string().trim().optional(), {
      short: "b",
      description: "Branch ref override (default: refs/heads/master, or release.defaultBranch from config)",
    }),
    from: option(z.coerce.number().int().positive().optional(), {
      short: "f",
      description: "From run ID override (defaults to latest prod-success run on the branch)",
    }),
    to: option(z.string().trim().optional(), {
      short: "t",
      description: "To commit SHA override (default: latest successful pipeline run on the branch)",
    }),
    area: option(z.string().trim().optional(), {
      short: "a",
      description: "Area filter from predefined configs",
    }),
    "prod-stage-name": option(z.string().trim().optional(), {
      description: "Prod stage identifier/name override (useful for CI without config)",
    }),
    "no-copy": option(z.coerce.boolean().default(false), {
      description: "Print only, do not copy changelog to clipboard",
    }),
    "post-webhook": option(z.string().trim().optional(), {
      description: "Post changelog to Microsoft Teams incoming webhook URL",
    }),
    channel: option(z.string().trim().optional(), {
      description: "Named Teams channel from release config",
    }),
    json: option(z.coerce.boolean().default(false), {
      short: "j",
      description: "Print machine-readable JSON",
    }),
  },
  handler: async ({ flags, positional, prompt }) => {
    try {
      const isInteractiveTerminal = Boolean(process.stdin.isTTY && process.stdout.isTTY);
      const isCiMode = isCiEnvironment() || !isInteractiveTerminal;
      const config = await withSpinner("Loading release config", () => loadConfig(), {
        silentFailure: true,
        silentSuccess: true,
      });
      const context = await withSpinner("Loading Azure DevOps context", () => getAzureContext(), {
        silentFailure: true,
        silentSuccess: true,
      });
      const prodStageCache: ProdStageCache = new Map();
      const defaultPipeline = (config.release?.defaultPipeline || "").trim();
      const configuredProdStageName = (config.release?.prodStageName || "").trim();
      const resolvedProdStageName = (flags["prod-stage-name"] || configuredProdStageName).trim();
      const configuredDefaultBranch = (config.release?.defaultBranch || "").trim();
      const branch = (flags.branch || configuredDefaultBranch || "refs/heads/master").trim();
      const branchDisplayName = branch.replace(/^refs\/heads\//i, "");
      const mergedAreaConfigs = mergeAreaConfigs(config.areas);
      const resolvedChannels = resolveChannels(config);

      let pipelineName = resolveStringArg(flags.pipeline, positional);
      if (!pipelineName) {
        pipelineName = defaultPipeline;
      }

      if (!pipelineName) {
        if (isInteractiveTerminal) {
          pipelineName = (
            await prompt.text("Pipeline name", {
              placeholder: defaultPipeline || "example-release-pipeline",
              fallbackValue: defaultPipeline,
              validate: (value) => (value.trim().length > 0 ? true : "Pipeline name is required"),
            })
          ).trim();
        }
      }

      if (!pipelineName) {
        throw new Error(
          "Missing pipeline name. Pass `--pipeline <name>` or configure `release.defaultPipeline` via `release configure`.",
        );
      }

      const pipeline = await withSpinner("Resolving pipeline", () => resolvePipelineByName(pipelineName), {
        silentFailure: true,
        silentSuccess: true,
      });
      let fromRun = await withSpinner(
        "Resolving source release run",
        () => resolveFromRun(pipeline.id, context.baseUrl, flags.from, prodStageCache, resolvedProdStageName, branch, branchDisplayName),
        {
          silentFailure: true,
          silentSuccess: true,
        },
      );
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
        targetRun = await withSpinner(`Resolving target ${branchDisplayName} run`, () => resolveLatestSucceededBranchRun(pipeline.id, branch), {
          silentFailure: true,
          silentSuccess: true,
        });
      }

      const toCommit = flags.to ? normalizeCommitSha(flags.to) : (targetRun?.sourceVersion || "").trim();

      let effectiveToCommit = toCommit;
      let toRunId = flags.to ? undefined : targetRun?.id;

      if (!effectiveToCommit) {
        throw new Error(`Could not resolve target commit from latest successful ${branchDisplayName} pipeline run.`);
      }

      if (fromCommit.toLowerCase() === effectiveToCommit.toLowerCase()) {
        const currentReleaseRun = targetRun || fromRun;
        const previousRun = await resolvePreviousProdReleaseRun(
          pipeline.id,
          context.baseUrl,
          currentReleaseRun.id,
          prodStageCache,
          resolvedProdStageName,
          branch,
        );
        if (!previousRun) {
          throw new Error(`No new ${branchDisplayName} commits and no previous successful prod release run found for fallback.`);
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

      const commits = await withSpinner(
        "Loading commit range",
        () => loadRemoteCommitRange({
          baseUrl: context.baseUrl,
          repositoryId,
          fromCommit,
          toCommit: effectiveToCommit,
        }),
        { silentFailure: true, silentSuccess: true },
      );

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

      if (isCiMode && !flags["post-webhook"] && !flags.channel) {
        throw new Error("Missing webhook target in CI mode. Pass `--post-webhook <url>` or `--channel <name>`.");
      }

      const webhookTarget = await resolveWebhookTarget(
        {
          adHocWebhookUrl: flags["post-webhook"],
          selectedChannel: flags.channel,
          channels: resolvedChannels,
          useInteractivePrompt: !flags.json && isInteractiveTerminal,
        },
        prompt as unknown as {
          select(
            message: string,
            options: {
              options: Array<{
                value: string;
                label: string;
                hint?: string;
              }>;
              default?: string;
            },
          ): Promise<string>;
        },
      );
      const webhookUrl = webhookTarget.webhookUrl;

      if (isCiMode && !webhookUrl) {
        throw new Error(
          "Missing Teams webhook in CI mode. Pass `--post-webhook <url>` or `--channel <name>` with configured channels.",
        );
      }

      let skippedWebhookPost = false;

      if (webhookUrl && !flags.json && isInteractiveTerminal) {
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
        postedFormat = await withSpinner(
          "Posting changelog to Teams",
          () =>
            postChangelogToTeams({
              webhookUrl,
              pipelineName: pipeline.name || pipelineName,
              fromRunId: fromRun.id,
              toRunId,
              fromCommit,
              toCommit: effectiveToCommit,
              area: areaName,
              pipelineRunUrl: toRunId ? buildRunUrl(context, toRunId) : undefined,
              markdown,
            }),
          { silentFailure: true, silentSuccess: true },
        );
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
                branch: branchDisplayName,
              },
              area: areaName || null,
              totalCommits: commits.length,
              includedCount: countGroupedCommits(grouped),
              manualReviewCount: manualReview.length,
              postedToWebhook: Boolean(webhookUrl) && !skippedWebhookPost && Boolean(postedFormat),
              postedFormat,
              postedChannel: webhookTarget.channelName || null,
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

      if (!flags["no-copy"] && isInteractiveTerminal) {
        await withSpinner("Copying changelog to clipboard", () => copyToClipboard(markdown), {
          silentFailure: true,
          silentSuccess: true,
        });
        printSuccess("Copied changelog to clipboard.");
      } else if (!flags["no-copy"]) {
        printInfo("Skipped clipboard copy in non-interactive mode.");
      }

      if (postedFormat) {
        const channelLabel = webhookTarget.channelName ? ` channel '${webhookTarget.channelName}'` : " webhook";
        printSuccess(`Posted changelog to Teams${channelLabel} (${postedFormat}).`);
      } else if (skippedWebhookPost) {
        printInfo("Skipped Teams webhook post.");
      }
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      printError(
        `Failed to build changelog: ${message}`,
        "Verify SYSTEM_ACCESSTOKEN/SYSTEM_COLLECTIONURI/SYSTEM_TEAMPROJECT in CI or local Azure defaults with `release configure --show`.",
      );
      process.exit(1);
    }
  },
});

async function resolveFromRun(
  pipelineId: number,
  baseUrl: string,
  fromRunId: number | undefined,
  prodStageCache: ProdStageCache,
  prodStageName: string | undefined,
  branch: string,
  branchDisplayName: string,
): Promise<PipelineRun> {
  if (fromRunId) {
    const run = await loadRunById(fromRunId);
    validateRunBranchSuccess(run, pipelineId, branchDisplayName);
    return run;
  }

  const latestProdReleaseRun = await resolveLatestProdReleaseRun(pipelineId, baseUrl, prodStageCache, prodStageName, branch);
  if (!latestProdReleaseRun) {
    throw new Error(`No successful prod release run found on ${branchDisplayName}. Configure release.prodStageName via \`release configure\` or pass --from <runId>.`);
  }

  return latestProdReleaseRun;
}

async function resolveLatestProdReleaseRun(
  pipelineId: number,
  baseUrl: string,
  prodStageCache: ProdStageCache,
  prodStageName: string | undefined,
  branch: string,
): Promise<PipelineRun | null> {
  const runs = await loadCompletedBranchRuns(pipelineId, branch, 1);
  if (runs.length === 1) {
    const isProdSuccess = await hasSuccessfulProdStage({
      baseUrl,
      runId: runs[0].id,
      cache: prodStageCache,
      prodStageName,
    });
    if (isProdSuccess) {
      return runs[0];
    }
  }

  const expandedRuns = await loadCompletedBranchRuns(pipelineId, branch, 200);
  for (const run of expandedRuns) {
    if (
      await hasSuccessfulProdStage({
        baseUrl,
        runId: run.id,
        cache: prodStageCache,
        prodStageName,
      })
    ) {
      return run;
    }
  }

  return null;
}

async function resolveLatestSucceededBranchRun(pipelineId: number, branch: string): Promise<PipelineRun | null> {
  const runs = await loadSucceededBranchRuns(pipelineId, branch, 1);
  return runs[0] || null;
}

async function resolvePreviousProdReleaseRun(
  pipelineId: number,
  baseUrl: string,
  currentRunId: number,
  prodStageCache: ProdStageCache,
  prodStageName: string | undefined,
  branch: string,
): Promise<PipelineRun | null> {
  const runs = await loadCompletedBranchRuns(pipelineId, branch, 200);
  let passedCurrent = false;

  for (const run of runs) {
    const isProdSuccess = await hasSuccessfulProdStage({
      baseUrl,
      runId: run.id,
      cache: prodStageCache,
      prodStageName,
    });
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

async function loadSucceededBranchRuns(pipelineId: number, branch: string, top: number): Promise<PipelineRun[]> {
  return loadPipelineRuns({
    pipelineId,
    status: "completed",
    result: "succeeded",
    branch,
    top,
  });
}

async function loadCompletedBranchRuns(pipelineId: number, branch: string, top: number): Promise<PipelineRun[]> {
  return loadPipelineRuns({
    pipelineId,
    status: "completed",
    branch,
    top,
  });
}

function validateRunBranchSuccess(run: PipelineRun, pipelineId: number, expectedBranch: string): void {
  if (run.definition?.id && run.definition.id !== pipelineId) {
    throw new Error(`Run #${run.id} does not belong to selected pipeline ID ${pipelineId}.`);
  }

  if ((run.status || "").toLowerCase() !== "completed") {
    throw new Error(`Run #${run.id} is not completed.`);
  }

  if ((run.result || "").toLowerCase() !== "succeeded") {
    throw new Error(`Run #${run.id} is not succeeded.`);
  }

  const runBranch = normalizeBranch(run.sourceBranch || "");
  if (runBranch !== expectedBranch.toLowerCase()) {
    throw new Error(`Run #${run.id} is on branch '${run.sourceBranch || ""}', expected ${expectedBranch}.`);
  }
}

async function resolveLatestBranchCommit(baseUrl: string, repositoryId: string, branch: string): Promise<string> {
  const client = await getAzureClientForBaseUrl(baseUrl);
  const response = await client.getJson<AzureGitCommitResponse>(`/_apis/git/repositories/${repositoryId}/commits`, {
    "searchCriteria.itemVersion.version": branch,
    "searchCriteria.itemVersion.versionType": "branch",
    "searchCriteria.historyMode": "firstParent",
    "searchCriteria.$top": 1,
  });

  return (response.value?.[0]?.commitId || "").trim();
}

function isCiEnvironment(): boolean {
  return ["CI", "TF_BUILD", "SYSTEM_ACCESSTOKEN", "SYSTEM_COLLECTIONURI", "SYSTEM_TEAMPROJECT"].some(
    (name) => (process.env[name] || "").trim().length > 0,
  );
}

async function loadRemoteCommitRange(options: {
  baseUrl: string;
  repositoryId: string;
  fromCommit: string;
  toCommit: string;
}): Promise<CommitInfo[]> {
  const client = await getAzureClientForBaseUrl(options.baseUrl);
  const pageSize = 100;
  const maxPages = 20;

  const loadRangeWithHistoryMode = async (historyMode?: "firstParent" | "fullHistory"): Promise<CommitInfo[] | null> => {
    const commits: CommitInfo[] = [];
    const seen = new Set<string>();
    let skip = 0;

    for (let page = 0; page < maxPages; page += 1) {
      const query: Record<string, string | number> = {
        "searchCriteria.itemVersion.version": options.toCommit,
        "searchCriteria.itemVersion.versionType": "commit",
        "searchCriteria.$top": pageSize,
        "searchCriteria.$skip": skip,
      };
      if (historyMode) {
        query["searchCriteria.historyMode"] = historyMode;
      }

      const response = await client.getJson<AzureGitCommitResponse>(
        `/_apis/git/repositories/${options.repositoryId}/commits`,
        query,
      );

      const items = response.value || [];
      if (items.length === 0) {
        return null;
      }

      for (const item of items) {
        const hash = (item.commitId || "").trim();
        if (!hash || seen.has(hash)) {
          continue;
        }
        seen.add(hash);

        if (hash.toLowerCase() === options.fromCommit.toLowerCase()) {
          return commits;
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

      skip += items.length;
    }

    return null;
  };

  const firstParentRange = await loadRangeWithHistoryMode("firstParent");
  if (firstParentRange) {
    return firstParentRange;
  }

  const fullHistoryRange = await loadRangeWithHistoryMode("fullHistory");
  if (fullHistoryRange) {
    return fullHistoryRange;
  }

  throw new Error(
    `Could not resolve commit range from ${options.fromCommit.slice(0, 7)} to ${options.toCommit.slice(0, 7)}. History may have diverged or been rewritten on the release branch. Pass --from <runId> from current branch history or set --to <sha>.`,
  );
}

async function getAzureClientForBaseUrl(baseUrl: string): Promise<Awaited<ReturnType<typeof getAzureClient>>["client"]> {
  const { client, context } = await getAzureClient();
  if (context.baseUrl !== baseUrl) {
    throw new Error(
      `Azure DevOps context mismatch. Expected '${baseUrl}', resolved '${context.baseUrl}'. Set SYSTEM_COLLECTIONURI and SYSTEM_TEAMPROJECT to align context.`,
    );
  }

  return client;
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

async function resolveWebhookTarget(
  options: {
    adHocWebhookUrl?: string;
    selectedChannel?: string;
    channels: Record<string, string>;
    useInteractivePrompt: boolean;
  },
  prompt: {
    select(
      message: string,
      options: {
        options: Array<{
          value: string;
          label: string;
          hint?: string;
        }>;
        default?: string;
      },
    ): Promise<string>;
  },
): Promise<{ webhookUrl: string; channelName: string }> {
  const adHocWebhookUrl = (options.adHocWebhookUrl || "").trim();
  if (adHocWebhookUrl) {
    return {
      webhookUrl: adHocWebhookUrl,
      channelName: "ad-hoc",
    };
  }

  const channelEntries = Object.entries(options.channels);
  if (channelEntries.length === 0) {
    return {
      webhookUrl: "",
      channelName: "",
    };
  }

  const selectedChannel = (options.selectedChannel || "").trim();
  if (selectedChannel) {
    const matchedEntry = channelEntries.find(([channelName]) => normalizeText(channelName) === normalizeText(selectedChannel));
    if (!matchedEntry) {
      const available = channelEntries.map(([name]) => name).sort().join(", ");
      throw new Error(`Unknown Teams channel '${selectedChannel}'. Available channels: ${available}`);
    }

    return {
      channelName: matchedEntry[0],
      webhookUrl: matchedEntry[1],
    };
  }

  if (channelEntries.length === 1) {
    return {
      channelName: channelEntries[0][0],
      webhookUrl: channelEntries[0][1],
    };
  }

  if (!options.useInteractivePrompt) {
    const available = channelEntries.map(([name]) => name).sort().join(", ");
    throw new Error(`Multiple Teams channels configured. Pass --channel <name>. Available channels: ${available}`);
  }

  const selectedValue = await prompt.select("Choose Teams channel", {
    options: channelEntries
      .map(([name, webhookUrl]) => ({
        value: name,
        label: name,
        hint: maskWebhookUrl(webhookUrl),
      }))
      .sort((left, right) => left.label.localeCompare(right.label)),
    default: channelEntries[0][0],
  });

  const selectedEntry = channelEntries.find(([name]) => name === selectedValue);
  if (!selectedEntry) {
    throw new Error(`Failed to resolve selected Teams channel '${selectedValue}'.`);
  }

  return {
    channelName: selectedEntry[0],
    webhookUrl: selectedEntry[1],
  };
}

function maskWebhookUrl(value: string): string {
  const trimmed = value.trim();
  if (trimmed.length <= 14) {
    return "***";
  }

  return `${trimmed.slice(0, 10)}...${trimmed.slice(-4)}`;
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
