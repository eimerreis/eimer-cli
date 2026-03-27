type CheckStatus = "pass" | "fail" | "pending" | "unknown";

type ShowReason = {
  summary: string;
  details?: string;
  url?: string;
};

type ShowResult = {
  platform: "github" | "azure-devops";
  pr: {
    id: number;
    title: string;
    url?: string;
    repository: string;
    branch?: string;
    targetBranch?: string;
    status?: string;
    isDraft?: boolean;
  };
  comments: {
    hasComments: boolean;
    threadCount: number;
    commentCount: number;
  };
  approval: {
    approved: boolean;
    summary: string;
  };
  checks: {
    status: CheckStatus;
    passed: number;
    failed: number;
    pending: number;
    details: Array<{ name: string; status: CheckStatus; url?: string }>;
  };
  readyToMerge: boolean;
  notReadyReasons: ShowReason[];
};

type GitHubPrDetail = {
  number: number;
  title: string;
  url?: string;
  state?: string;
  isDraft?: boolean;
  reviewDecision?: string | null;
  mergeable?: string;
  mergeStateStatus?: string;
  headRefName?: string;
  baseRefName?: string;
};

type GitHubCheck = {
  name?: string;
  bucket?: string;
  state?: string;
  link?: string;
  workflow?: string;
};

type AzurePrDetail = {
  pullRequestId: number;
  title: string;
  url?: string;
  status?: string;
  isDraft?: boolean;
  mergeStatus?: string;
  sourceRefName?: string;
  targetRefName?: string;
  repository?: {
    name?: string;
    webUrl?: string;
  };
  reviewers?: Array<{
    displayName?: string;
    isContainer?: boolean;
    vote?: number;
  }>;
};

type AzurePolicy = {
  status?: string;
  configuration?: {
    isBlocking?: boolean;
    type?: {
      displayName?: string;
    };
  };
  context?: {
    buildId?: number;
    buildDefinitionName?: string;
  };
};

async function runJsonAllowExitCodes<T>(command: string[], allowedExitCodes: number[]): Promise<T> {
  const process = Bun.spawn({
    cmd: command,
    stdout: "pipe",
    stderr: "pipe",
  });

  const [stdout, stderr, exitCode] = await Promise.all([
    new Response(process.stdout).text(),
    new Response(process.stderr).text(),
    process.exited,
  ]);

  if (!allowedExitCodes.includes(exitCode)) {
    throw new Error(`Command failed (${command.join(" ")}): ${stderr.trim() || stdout.trim()}`);
  }

  try {
    return JSON.parse(stdout) as T;
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    throw new Error(`Failed to parse JSON from '${command.join(" ")}': ${message}`);
  }
}

function normalizeGitHubCheckStatus(bucket?: string, state?: string): CheckStatus {
  const normalizedBucket = (bucket || "").toLowerCase();
  if (normalizedBucket === "pass") {
    return "pass";
  }

  if (normalizedBucket === "fail" || normalizedBucket === "cancel") {
    return "fail";
  }

  if (normalizedBucket === "pending") {
    return "pending";
  }

  const normalizedState = (state || "").toLowerCase();
  if (["success", "succeeded", "completed"].includes(normalizedState)) {
    return "pass";
  }

  if (["failure", "failed", "error", "cancelled", "canceled"].includes(normalizedState)) {
    return "fail";
  }

  if (["queued", "pending", "in_progress", "in progress", "requested", "waiting"].includes(normalizedState)) {
    return "pending";
  }

  return "unknown";
}

function normalizeAzurePolicyStatus(status?: string): CheckStatus {
  const normalized = (status || "").toLowerCase();

  if (normalized === "approved") {
    return "pass";
  }

  if (normalized === "rejected") {
    return "fail";
  }

  if (["queued", "running", "pending", "notset"].includes(normalized)) {
    return "pending";
  }

  return "unknown";
}

function summarizeCheckStatus(passed: number, failed: number, pending: number, total: number): CheckStatus {
  if (failed > 0) {
    return "fail";
  }

  if (pending > 0) {
    return "pending";
  }

  if (passed > 0 || total === 0) {
    return "pass";
  }

  return "unknown";
}

function summarizeAzureChecks(
  policies: AzurePolicy[],
  repoWebUrl: string,
  remoteUrl: string,
): ShowResult["checks"] {
  const projectBaseUrl = parseAzureProjectBaseUrl(repoWebUrl, remoteUrl);
  const buildPolicies = policies.filter((policy) => (policy.configuration?.type?.displayName || "").toLowerCase() === "build");

  const details = buildPolicies.map((policy) => {
    const name = policy.context?.buildDefinitionName || "Build";
    const buildId = policy.context?.buildId;
    const url = buildId && projectBaseUrl ? `${projectBaseUrl}/_build/results?buildId=${buildId}` : "";

    return {
      name: buildId ? `${name} (#${buildId})` : name,
      status: normalizeAzurePolicyStatus(policy.status),
      url: url || undefined,
    };
  });

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

function summarizeAzureApproval(
  pr: AzurePrDetail,
  policies: AzurePolicy[],
): ShowResult["approval"] {
  const reviewers = pr.reviewers || [];
  const approverNames = reviewers
    .filter((reviewer) => (reviewer.vote || 0) >= 5)
    .map((reviewer) => reviewer.displayName || "Unknown")
    .filter((name) => name && name.trim().length > 0);

  const reviewerPolicyApproved = policies.some(
    (policy) =>
      (policy.configuration?.type?.displayName || "").toLowerCase() === "minimum number of reviewers" &&
      (policy.status || "").toLowerCase() === "approved",
  );

  const approved = approverNames.length > 0 || reviewerPolicyApproved;
  const summary = approved
    ? approverNames.length > 0
      ? `Approved by ${approverNames.join(", ")}`
      : "Reviewer policy approved"
    : "No approvals yet";

  return { approved, summary };
}

function collectGitHubNotReadyReasons(
  detail: GitHubPrDetail,
  approved: boolean,
  checks: ShowResult["checks"],
): ShowReason[] {
  const reasons: ShowReason[] = [];

  if ((detail.state || "").toUpperCase() !== "OPEN") {
    reasons.push({ summary: `PR state is '${detail.state || "unknown"}'` });
  }

  if (detail.isDraft) {
    reasons.push({ summary: "PR is marked as draft" });
  }

  if ((detail.mergeable || "").toUpperCase() === "CONFLICTING") {
    reasons.push({ summary: "PR has merge conflicts" });
  }

  if (!approved) {
    reasons.push({ summary: "PR is not approved" });
  }

  if (checks.status === "fail") {
    for (const failed of checks.details.filter((item) => item.status === "fail")) {
      reasons.push({ summary: `Check failed: ${failed.name}`, url: failed.url });
    }
  }

  if (checks.status === "pending") {
    const pendingNames = checks.details
      .filter((item) => item.status === "pending")
      .map((item) => item.name)
      .slice(0, 3);
    reasons.push({ summary: "Checks are still pending", details: pendingNames.join(", ") || undefined });
  }

  const mergeStateStatus = (detail.mergeStateStatus || "").toUpperCase();
  if (mergeStateStatus === "BLOCKED" || mergeStateStatus === "DIRTY") {
    reasons.push({ summary: `Merge state is ${mergeStateStatus.toLowerCase()}` });
  }

  return dedupeReasons(reasons);
}

function collectAzureNotReadyReasons(
  detail: AzurePrDetail,
  approved: boolean,
  checks: ShowResult["checks"],
  policies: AzurePolicy[],
): ShowReason[] {
  const reasons: ShowReason[] = [];

  if ((detail.status || "").toLowerCase() !== "active") {
    reasons.push({ summary: `PR status is '${detail.status || "unknown"}'` });
  }

  if (detail.isDraft) {
    reasons.push({ summary: "PR is marked as draft" });
  }

  if ((detail.mergeStatus || "").toLowerCase() === "conflicts") {
    reasons.push({ summary: "PR has merge conflicts" });
  }

  if (!approved) {
    reasons.push({ summary: "PR is not approved" });
  }

  if (checks.status === "fail") {
    for (const failed of checks.details.filter((item) => item.status === "fail")) {
      reasons.push({ summary: `Build failed: ${failed.name}`, url: failed.url });
    }
  }

  if (checks.status === "pending") {
    const pendingNames = checks.details
      .filter((item) => item.status === "pending")
      .map((item) => item.name)
      .slice(0, 3);
    reasons.push({ summary: "Build checks are still pending", details: pendingNames.join(", ") || undefined });
  }

  for (const policy of policies) {
    const displayName = policy.configuration?.type?.displayName || "Policy";
    const blocking = Boolean(policy.configuration?.isBlocking);
    const normalizedStatus = (policy.status || "").toLowerCase();

    if (!blocking || displayName.toLowerCase() === "build") {
      continue;
    }

    if (normalizedStatus === "approved" || normalizedStatus === "notapplicable") {
      continue;
    }

    reasons.push({ summary: `Blocking policy not satisfied: ${displayName}`, details: `status=${policy.status || "unknown"}` });
  }

  return dedupeReasons(reasons);
}

function dedupeReasons(reasons: ShowReason[]): ShowReason[] {
  const seen = new Set<string>();
  const deduped: ShowReason[] = [];
  for (const reason of reasons) {
    const key = `${reason.summary}|${reason.details || ""}|${reason.url || ""}`;
    if (seen.has(key)) {
      continue;
    }
    seen.add(key);
    deduped.push(reason);
  }
  return deduped;
}

function parseAzureProjectBaseUrl(repoWebUrl: string, remoteUrl: string): string {
  const fromRepoWebUrl = repoWebUrl.match(/^(https:\/\/dev\.azure\.com\/[^/]+\/[^/]+)\/_git\//i)?.[1];
  if (fromRepoWebUrl) {
    return fromRepoWebUrl;
  }

  const httpsMatch = remoteUrl.match(/^https:\/\/dev\.azure\.com\/([^/]+)\/([^/]+)\/_git\//i);
  if (httpsMatch) {
    return `https://dev.azure.com/${httpsMatch[1]}/${httpsMatch[2]}`;
  }

  const sshMatch = remoteUrl.match(/^(?:ssh:\/\/)?git@ssh\.dev\.azure\.com:v3\/([^/]+)\/([^/]+)\/[^/]+(?:\.git)?$/i);
  if (sshMatch) {
    return `https://dev.azure.com/${sshMatch[1]}/${sshMatch[2]}`;
  }

  const vsSshMatch = remoteUrl.match(/^(?:ssh:\/\/)?[^@]+@([^.]+)\.vs-ssh\.visualstudio\.com:v3\/([^/]+)\/[^/]+(?:\.git)?$/i);
  if (vsSshMatch) {
    return `https://${vsSshMatch[1]}.visualstudio.com/${vsSshMatch[2]}`;
  }

  return "";
}

function buildAzurePrUrl(pr: AzurePrDetail): string {
  if (pr.repository?.webUrl) {
    return `${pr.repository.webUrl}/pullrequest/${pr.pullRequestId}`;
  }

  return "";
}

function stripAzureRefPrefix(branch?: string): string {
  if (!branch) {
    return "";
  }

  return branch.startsWith("refs/heads/") ? branch.slice("refs/heads/".length) : branch;
}

function printShowResult(result: ShowResult): void {
  console.log(`PR #${result.pr.id}: ${result.pr.title}`);
  console.log(`Platform: ${result.platform}`);
  console.log(`Repository: ${result.pr.repository}`);
  if (result.pr.branch || result.pr.targetBranch) {
    console.log(`Branch: ${result.pr.branch || "?"} -> ${result.pr.targetBranch || "?"}`);
  }
  if (result.pr.url) {
    console.log(`URL: ${result.pr.url}`);
  }

  console.log(
    `Comments: ${result.comments.hasComments ? "YES" : "NO"} (${result.comments.commentCount} comments in ${result.comments.threadCount} threads)`,
  );
  console.log(`Approved: ${result.approval.approved ? "YES" : "NO"} (${result.approval.summary})`);
  console.log(
    `Checks: ${result.checks.status.toUpperCase()} (${result.checks.passed} passed, ${result.checks.failed} failed, ${result.checks.pending} pending)`,
  );

  if (result.checks.details.length > 0) {
    const nonPassing = result.checks.details.filter((item) => item.status !== "pass");
    if (nonPassing.length > 0) {
      console.log("Check details:");
      for (const check of nonPassing) {
        console.log(`- ${check.status.toUpperCase()}: ${check.name}${check.url ? ` | ${check.url}` : ""}`);
      }
    }
  }

  if (result.readyToMerge) {
    console.log("Ready to merge: YES");
    return;
  }

  console.log("Ready to merge: NO");
  console.log("Why not ready:");
  for (const reason of result.notReadyReasons) {
    const detailsSuffix = reason.details ? ` (${reason.details})` : "";
    const urlSuffix = reason.url ? ` | ${reason.url}` : "";
    console.log(`- ${reason.summary}${detailsSuffix}${urlSuffix}`);
  }
}

export {
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
};

export type { AzurePolicy, AzurePrDetail, CheckStatus, GitHubCheck, GitHubPrDetail, ShowResult };
