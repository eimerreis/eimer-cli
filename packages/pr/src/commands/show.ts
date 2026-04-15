import { defineCommand, option } from "@bunli/core";
import { printError, printInfo, withSpinner } from "@scripts/ui";
import { z } from "zod";
import { findAzurePrByBranch, findAzurePrById, loadAzureComments } from "./comments-azdo";
import { findGitHubPrByBranch, findGitHubPrById, loadGitHubComments, parseGitHubRepo } from "./comments-github";
import { detectPlatform, resolveIdArg, runJson, runText, type AzurePullRequest, type GitHubRepo } from "./comments-utils";
import {
  buildAzurePrUrl,
  collectAzureNotReadyReasons,
  collectGitHubNotReadyReasons,
  normalizeGitHubCheckStatus,
  printShowResult,
  runJsonAllowExitCodes,
  stripAzureRefPrefix,
  summarizeAzureApproval,
  summarizeAzureChecks,
  summarizeCheckStatus,
  type AzurePolicy,
  type AzurePrDetail,
  type GitHubCheck,
  type GitHubPrDetail,
  type ShowResult,
} from "./show-helpers";

const showCommand = defineCommand({
  name: "show",
  description: "Show PR merge readiness details",
  options: {
    id: option(z.coerce.number().int().positive().optional(), {
      short: "i",
      description: "Pull request ID/number override",
    }),
    branch: option(z.string().trim().optional(), {
      short: "b",
      description: "Branch override for PR lookup",
    }),
    repo: option(z.string().trim().optional(), {
      short: "r",
      description: "Repo override (GitHub: owner/repo, AzDO: repo name)",
    }),
    json: option(z.coerce.boolean().default(false), {
      short: "j",
      description: "Print machine-readable JSON",
    }),
  },
  handler: async ({ flags, positional, prompt }) => {
    try {
      const remoteUrl = (await runText(["git", "remote", "get-url", "origin"])).trim();
      const platform = detectPlatform(remoteUrl);
      const prId = resolveIdArg(flags.id, positional);
      let branch = flags.branch?.trim() || "";

      if (!branch && !prId) {
        branch = (await runText(["git", "branch", "--show-current"])).trim();
      }

      if (!branch && !prId) {
        branch = (
          await prompt.text("Branch name", {
            placeholder: "feature/my-branch",
            fallbackValue: "",
            validate: (value) => (value.trim().length > 0 ? true : "Branch is required"),
          })
        ).trim();

        if (!branch) {
          printError("Could not determine branch. Pass [id], --branch, or --id.", "Run from a checked out branch or pass --branch explicitly.");
          process.exit(1);
        }
      }

      const result = await withSpinner(
        "Loading pull request details",
        () =>
          platform === "github"
            ? showGitHubPr(remoteUrl, branch, prId, flags.repo)
            : showAzurePr(remoteUrl, branch, prId, flags.repo),
        { silentFailure: true },
      );

      if (!result) {
        printInfo(`No pull request found for branch '${branch}'.`, "Try `pr list --all` or pass --id / --repo to target a different PR.");
        return;
      }

      if (flags.json) {
        console.log(JSON.stringify(result, null, 2));
        return;
      }

      printShowResult(result);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      printError(`Failed to show PR: ${message}`, "Check your git remote and authenticate with `gh auth status` or `az login`.");
      process.exit(1);
    }
  },
});

async function showGitHubPr(
  remoteUrl: string,
  branch: string,
  providedPrId?: number,
  repoOverride?: string,
): Promise<ShowResult | null> {
  const repo = parseGitHubRepo(remoteUrl, repoOverride);
  const pr = providedPrId ? await findGitHubPrById(repo, providedPrId) : await tryFindGitHubPrByBranch(repo, branch);

  if (!pr) {
    return null;
  }

  const detail = await runJson<GitHubPrDetail>([
    "gh",
    "pr",
    "view",
    String(pr.number),
    "--repo",
    `${repo.owner}/${repo.name}`,
    "--json",
    "number,title,url,state,isDraft,reviewDecision,mergeable,mergeStateStatus,headRefName,baseRefName",
  ]);

  const commentsResult = await loadGitHubComments(remoteUrl, detail.headRefName || branch, pr.number, repoOverride);
  const commentCount = commentsResult.threads.reduce((total, thread) => total + thread.comments.length, 0);

  const checks = await loadGitHubChecks(repo, detail.number);
  const approvalApproved = (detail.reviewDecision || "").toUpperCase() === "APPROVED";
  const reasons = collectGitHubNotReadyReasons(detail, approvalApproved, checks);

  return {
    platform: "github",
    pr: {
      id: detail.number,
      title: detail.title,
      url: detail.url,
      repository: `${repo.owner}/${repo.name}`,
      branch: detail.headRefName || branch,
      targetBranch: detail.baseRefName,
      status: detail.state,
      isDraft: Boolean(detail.isDraft),
    },
    comments: {
      hasComments: commentCount > 0,
      threadCount: commentsResult.threads.length,
      commentCount,
    },
    approval: {
      approved: approvalApproved,
      summary: detail.reviewDecision || "REVIEW_REQUIRED",
    },
    checks,
    readyToMerge: reasons.length === 0,
    notReadyReasons: reasons,
  };
}

async function showAzurePr(
  remoteUrl: string,
  branch: string,
  providedPrId?: number,
  repoOverride?: string,
): Promise<ShowResult | null> {
  const pr = providedPrId ? await findAzurePrById(providedPrId) : await tryFindAzurePrByBranch(branch, repoOverride);
  if (!pr) {
    return null;
  }

  const detail = await runJson<AzurePrDetail>([
    "az",
    "repos",
    "pr",
    "show",
    "--id",
    String(pr.pullRequestId),
    "--detect",
    "true",
    "--output",
    "json",
  ]);

  const normalizedBranch = stripAzureRefPrefix(detail.sourceRefName || branch);
  const commentsResult = await loadAzureComments(normalizedBranch, detail.pullRequestId, repoOverride);
  const commentCount = commentsResult.threads.reduce((total, thread) => total + thread.comments.length, 0);

  const policies = await runJson<AzurePolicy[]>([
    "az",
    "repos",
    "pr",
    "policy",
    "list",
    "--id",
    String(detail.pullRequestId),
    "--detect",
    "true",
    "--output",
    "json",
  ]);

  const checks = summarizeAzureChecks(policies, detail.repository?.webUrl || "", remoteUrl);
  const approval = summarizeAzureApproval(detail, policies);
  const reasons = collectAzureNotReadyReasons(detail, approval.approved, checks, policies);

  return {
    platform: "azure-devops",
    pr: {
      id: detail.pullRequestId,
      title: detail.title,
      url: buildAzurePrUrl(detail),
      repository: detail.repository?.name || repoOverride || "unknown",
      branch: normalizedBranch,
      targetBranch: stripAzureRefPrefix(detail.targetRefName),
      status: detail.status,
      isDraft: Boolean(detail.isDraft),
    },
    comments: {
      hasComments: commentCount > 0,
      threadCount: commentsResult.threads.length,
      commentCount,
    },
    approval,
    checks,
    readyToMerge: reasons.length === 0,
    notReadyReasons: reasons,
  };
}

async function loadGitHubChecks(repo: GitHubRepo, prNumber: number): Promise<ShowResult["checks"]> {
  const checks = await runJsonAllowExitCodes<GitHubCheck[]>(
    [
      "gh",
      "pr",
      "checks",
      String(prNumber),
      "--repo",
      `${repo.owner}/${repo.name}`,
      "--json",
      "name,state,bucket,link,workflow",
    ],
    [0, 1, 8],
  );

  const details = (checks || []).map((check) => ({
    name: check.name || check.workflow || "Unnamed check",
    status: normalizeGitHubCheckStatus(check.bucket, check.state),
    url: check.link,
  }));

  const failed = details.filter((item) => item.status === "fail").length;
  const pending = details.filter((item) => item.status === "pending").length;
  const passed = details.filter((item) => item.status === "pass").length;

  return {
    status: summarizeCheckStatus(passed, failed, pending, details.length),
    passed,
    failed,
    pending,
    details,
  };
}

async function tryFindGitHubPrByBranch(repo: GitHubRepo, branch: string) {
  try {
    return await findGitHubPrByBranch(repo, branch);
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    if (message.includes("No active GitHub PR found")) {
      return null;
    }
    throw error;
  }
}

async function tryFindAzurePrByBranch(branch: string, repoOverride?: string): Promise<AzurePullRequest | null> {
  try {
    return await findAzurePrByBranch(branch, repoOverride);
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    if (message.includes("No active Azure DevOps PR found")) {
      return null;
    }
    throw error;
  }
}

export default showCommand;
