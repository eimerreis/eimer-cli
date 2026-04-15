import { printWarning } from "@scripts/ui";
import type { AzurePullRequest, AzureThread, NormalizedResult } from "./comments-utils";
import { AZURE_DEVOPS_RESOURCE, buildLocation, mapAzureStatus, runJson } from "./comments-utils";

async function loadAzureComments(
  branch: string,
  providedPrId?: number,
  repoOverride?: string,
): Promise<NormalizedResult> {
  const pr = providedPrId
    ? await runJson<AzurePullRequest>([
        "az",
        "repos",
        "pr",
        "show",
        "--id",
        String(providedPrId),
        "--detect",
        "true",
        "--output",
        "json",
      ])
    : await findAzurePrByBranch(branch, repoOverride);

  if (!pr.url) {
    throw new Error("Azure PR payload did not include a URL. Cannot fetch comment threads.");
  }

  const token = await getAzureDevopsToken();
  const threadsResponse = await fetch(`${pr.url}/threads?api-version=7.1`, {
    method: "GET",
    headers: {
      Authorization: `Bearer ${token}`,
      Accept: "application/json",
    },
  });

  if (!threadsResponse.ok) {
    const body = await threadsResponse.text();
    throw new Error(`Failed to fetch Azure PR threads (${threadsResponse.status}): ${body}`);
  }

  const payload = (await threadsResponse.json()) as { value?: AzureThread[] };
  const threads = normalizeAzureThreads(payload.value || []);

  return {
    platform: "azure-devops",
    pr: {
      id: pr.pullRequestId,
      title: pr.title,
      branch: pr.sourceRefName || branch,
      repository: pr.repository?.name || repoOverride,
    },
    threads,
  };
}

async function findAzurePrById(prId: number): Promise<AzurePullRequest | null> {
  try {
    return await runJson<AzurePullRequest>([
      "az",
      "repos",
      "pr",
      "show",
      "--id",
      String(prId),
      "--detect",
      "true",
      "--output",
      "json",
    ]);
  } catch {
    return null;
  }
}

async function findAzurePrByBranch(branch: string, repoOverride?: string): Promise<AzurePullRequest> {
  const baseCommand = [
    "az",
    "repos",
    "pr",
    "list",
    "--status",
    "active",
    "--top",
    "20",
    "--detect",
    "true",
    "--output",
    "json",
  ];

  if (repoOverride) {
    baseCommand.push("--repository", repoOverride);
  }

  const withShortBranch = await runJson<AzurePullRequest[]>([
    ...baseCommand,
    "--source-branch",
    branch,
  ]);

  const prs = withShortBranch.length
    ? withShortBranch
    : await runJson<AzurePullRequest[]>([
        ...baseCommand,
        "--source-branch",
        `refs/heads/${branch}`,
      ]);

  if (prs.length === 0) {
    throw new Error(`No active Azure DevOps PR found for branch '${branch}'. Use --id to target a PR directly.`);
  }

  const sorted = [...prs].sort((a, b) => {
    const aTime = Date.parse(a.creationDate || "") || 0;
    const bTime = Date.parse(b.creationDate || "") || 0;
    return bTime - aTime;
  });

  if (sorted.length > 1) {
    printWarning(`Found ${sorted.length} Azure PRs for branch '${branch}'. Using most recent (#${sorted[0].pullRequestId}).`);
  }

  return sorted[0];
}

async function getAzureDevopsToken(): Promise<string> {
  const tokenResponse = await runJson<{ accessToken?: string }>([
    "az",
    "account",
    "get-access-token",
    "--resource",
    AZURE_DEVOPS_RESOURCE,
    "--output",
    "json",
  ]);

  if (!tokenResponse.accessToken) {
    throw new Error("Failed to acquire Azure DevOps access token from Azure CLI.");
  }

  return tokenResponse.accessToken;
}

function normalizeAzureThreads(threads: AzureThread[]) {
  const reviewThreads = threads.filter((thread) => {
    if (thread.isDeleted) {
      return false;
    }

    return (thread.comments || []).some(
      (comment) => !comment.isDeleted && comment.commentType?.toLowerCase() === "text",
    );
  });

  return reviewThreads.map((thread) => {
    const line = thread.threadContext?.rightFileStart?.line;
    const filePath = thread.threadContext?.filePath;
    const location = buildLocation(filePath, line);

    const comments = (thread.comments || [])
      .filter((comment) => !comment.isDeleted && comment.commentType?.toLowerCase() === "text")
      .map((comment) => ({
        id: comment.id,
        parentCommentId: comment.parentCommentId,
        author: comment.author?.displayName || "Unknown",
        body: (comment.content || "").trim(),
        createdAt: comment.publishedDate,
      }));

    return {
      id: thread.id,
      status: mapAzureStatus(thread.status),
      filePath,
      line,
      location,
      comments,
      updatedAt: thread.lastUpdatedDate,
    };
  });
}

export { loadAzureComments };
export { findAzurePrByBranch, findAzurePrById };
