import { defineCommand, option } from "@bunli/core";
import { z } from "zod";
import { parseGitHubRepo } from "./comments-github";
import { detectPlatform, runJson, runText } from "./comments-utils";

type ListItem = {
  platform: "github" | "azure-devops";
  id: number;
  title: string;
  url: string;
  repository: string;
  branch?: string;
  updatedAt?: string;
  author?: string;
  reviewerVote?: number;
};

type GitHubPrListItem = {
  number: number;
  title: string;
  url?: string;
  headRefName?: string;
  updatedAt?: string;
  createdAt?: string;
  repository?: {
    nameWithOwner?: string;
  };
};

type AzureReviewer = {
  displayName?: string;
  uniqueName?: string;
  vote?: number;
};

type AzurePrListItem = {
  pullRequestId: number;
  title: string;
  sourceRefName?: string;
  creationDate?: string;
  closedDate?: string;
  repository?: {
    name?: string;
    webUrl?: string;
  };
  url?: string;
  createdBy?: {
    displayName?: string;
    uniqueName?: string;
  };
  reviewers?: AzureReviewer[];
};

const listCommand = defineCommand({
  name: "list",
  description: "List open pull requests",
  options: {
    json: option(z.coerce.boolean().default(false), {
      short: "j",
      description: "Print machine-readable JSON",
    }),
    all: option(z.coerce.boolean().default(false), {
      short: "a",
      description: "Search across all repos",
    }),
    reviewer: option(z.coerce.boolean().default(false), {
      short: "r",
      description: "Show PRs where I am a reviewer",
    }),
    top: option(z.coerce.number().int().positive().default(50), {
      short: "t",
      description: "Maximum number of PRs to fetch",
    }),
  },
  handler: async ({ flags, prompt }) => {
    try {
      const remoteUrl = (await runText(["git", "remote", "get-url", "origin"])) .trim();
      const platform = detectPlatform(remoteUrl);

      const items =
        platform === "github"
          ? await listGitHubPrs(remoteUrl, flags.all, flags.reviewer, flags.top)
          : await listAzurePrs(remoteUrl, flags.all, flags.reviewer, flags.top, prompt);

      if (flags.json) {
        console.log(JSON.stringify(items, null, 2));
        return;
      }

      printList(items, flags.reviewer, flags.all);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      console.error(`Failed to list pull requests: ${message}`);
      process.exit(1);
    }
  },
});

async function listGitHubPrs(
  remoteUrl: string,
  allRepos: boolean,
  reviewerMode: boolean,
  top: number,
): Promise<ListItem[]> {
  if (allRepos) {
    const query = reviewerMode ? "state:open review-requested:@me" : "state:open author:@me";
    const prs = await runJson<GitHubPrListItem[]>([
      "gh",
      "search",
      "prs",
      query,
      "--limit",
      String(top),
      "--json",
      "number,title,url,updatedAt,repository",
    ]);

    return prs
      .map((pr) => ({
        platform: "github" as const,
        id: pr.number,
        title: pr.title,
        url: pr.url || "",
        repository: pr.repository?.nameWithOwner || "unknown/unknown",
        updatedAt: pr.updatedAt || pr.createdAt,
      }))
      .filter((pr) => Boolean(pr.url));
  }

  const repo = parseGitHubRepo(remoteUrl);
  const command = reviewerMode
    ? [
        "gh",
        "pr",
        "list",
        "--repo",
        `${repo.owner}/${repo.name}`,
        "--state",
        "open",
        "--search",
        "review-requested:@me",
        "--json",
        "number,title,url,headRefName,updatedAt",
        "--limit",
        String(top),
      ]
    : [
        "gh",
        "pr",
        "list",
        "--repo",
        `${repo.owner}/${repo.name}`,
        "--author",
        "@me",
        "--state",
        "open",
        "--json",
        "number,title,url,headRefName,updatedAt",
        "--limit",
        String(top),
      ];

  const prs = await runJson<GitHubPrListItem[]>(command);
  return prs
    .map((pr) => ({
      platform: "github" as const,
      id: pr.number,
      title: pr.title,
      url: pr.url || "",
      repository: `${repo.owner}/${repo.name}`,
      branch: pr.headRefName,
      updatedAt: pr.updatedAt || pr.createdAt,
    }))
    .filter((pr) => Boolean(pr.url));
}

async function listAzurePrs(
  remoteUrl: string,
  allRepos: boolean,
  reviewerMode: boolean,
  top: number,
  prompt: { text(message: string, options?: { fallbackValue?: string; placeholder?: string }): Promise<string> },
): Promise<ListItem[]> {
  const azureIdentity = await resolveAzureIdentity(prompt);
  if (!azureIdentity) {
    throw new Error("Could not determine Azure DevOps user identity.");
  }

  const repoName = allRepos ? "" : parseRepoName(remoteUrl);
  const baseUrl = parseAzureProjectBaseUrl(remoteUrl);

  const command = [
    "az",
    "repos",
    "pr",
    "list",
    "--status",
    "active",
    "--top",
    String(top),
    "--detect",
    "true",
    "--output",
    "json",
  ];

  if (repoName) {
    command.push("--repository", repoName);
  }

  const prs = await runJson<AzurePrListItem[]>(command);
  const filtered = reviewerMode
    ? prs.filter((pr) => (pr.reviewers || []).some((reviewer) => matchesIdentity(reviewer.uniqueName, azureIdentity)))
    : prs.filter((pr) => matchesIdentity(pr.createdBy?.uniqueName, azureIdentity));

  return filtered
    .sort((a, b) => {
      const aTime = Date.parse(a.creationDate || "") || 0;
      const bTime = Date.parse(b.creationDate || "") || 0;
      return bTime - aTime;
    })
    .map((pr) => ({
      platform: "azure-devops" as const,
      id: pr.pullRequestId,
      title: pr.title,
      url: buildAzurePrUrl(pr, baseUrl),
      repository: pr.repository?.name || repoName || "unknown",
      branch: stripAzureRefPrefix(pr.sourceRefName),
      updatedAt: pr.creationDate || pr.closedDate,
      author: pr.createdBy?.displayName || pr.createdBy?.uniqueName,
      reviewerVote: reviewerMode ? findReviewerVote(pr.reviewers || [], azureIdentity) : undefined,
    }))
    .filter((pr) => Boolean(pr.url));
}

async function resolveAzureIdentity(
  prompt: { text(message: string, options?: { fallbackValue?: string; placeholder?: string }): Promise<string> },
): Promise<string> {
  const account = await runJson<{ user?: { name?: string } }>(["az", "account", "show", "--output", "json"]);
  const identity = (account.user?.name || "").trim();
  if (identity) {
    return identity;
  }

  const promptedIdentity = (
    await prompt.text("Azure DevOps user email", {
      placeholder: "your.name@example.com",
      fallbackValue: "",
    })
  ).trim();

  if (!promptedIdentity) {
    throw new Error(
      "Missing Azure identity. Re-run with interactive terminal or ensure 'az account show' includes user.name.",
    );
  }

  return promptedIdentity;
}

function parseRepoName(remoteUrl: string): string {
  const match = remoteUrl.match(/[:/]([^/:]+?)(?:\.git)?$/);
  const repoName = match?.[1];
  if (!repoName) {
    throw new Error(`Could not parse repository name from remote URL: ${remoteUrl}`);
  }

  return repoName;
}

function parseAzureProjectBaseUrl(remoteUrl: string): string {
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

function stripAzureRefPrefix(branch?: string): string {
  if (!branch) {
    return "";
  }

  return branch.startsWith("refs/heads/") ? branch.slice("refs/heads/".length) : branch;
}

function buildAzurePrUrl(pr: AzurePrListItem, baseUrl: string): string {
  if (pr.repository?.webUrl) {
    return `${pr.repository.webUrl}/pullrequest/${pr.pullRequestId}`;
  }

  if (baseUrl && pr.repository?.name) {
    return `${baseUrl}/_git/${pr.repository.name}/pullrequest/${pr.pullRequestId}`;
  }

  return "";
}

function matchesIdentity(candidate: string | undefined, identity: string): boolean {
  if (!candidate) {
    return false;
  }

  return candidate.toLowerCase() === identity.toLowerCase();
}

function findReviewerVote(reviewers: AzureReviewer[], identity: string): number {
  const reviewer = reviewers.find((item) => matchesIdentity(item.uniqueName, identity));
  return reviewer?.vote || 0;
}

function reviewerVoteLabel(vote?: number): string {
  if (vote === 10) {
    return "APPROVED";
  }

  if (vote === 5) {
    return "APPROVED-WITH-SUGGESTIONS";
  }

  if (vote === -5) {
    return "WAITING-FOR-AUTHOR";
  }

  if (vote === -10) {
    return "REJECTED";
  }

  return "NO-VOTE";
}

function terminalLink(text: string, url: string): string {
  return `\u001b]8;;${url}\u0007${text}\u001b]8;;\u0007`;
}

function printList(items: ListItem[], reviewerMode: boolean, allRepos: boolean): void {
  if (items.length === 0) {
    const mode = reviewerMode ? "reviewer" : "author";
    const scope = allRepos ? "all repos" : "current repo";
    console.log(`No open PRs found for ${mode} mode in ${scope}.`);
    return;
  }

  for (const item of items) {
    const reviewPrefix = reviewerMode ? `[${reviewerVoteLabel(item.reviewerVote)}] ` : "";
    const branch = item.branch ? ` | ${item.branch}` : "";
    const updatedAt = item.updatedAt ? ` | ${formatRelativeTime(item.updatedAt)}` : "";
    const line = `${reviewPrefix}#${item.id} ${item.title} | ${item.repository}${branch}${updatedAt}`;
    console.log(terminalLink(line, item.url));
  }
}

function formatRelativeTime(value: string): string {
  const timestamp = Date.parse(value);
  if (!Number.isFinite(timestamp)) {
    return value;
  }

  const deltaSeconds = Math.max(0, Math.floor((Date.now() - timestamp) / 1000));
  if (deltaSeconds < 60) {
    return "just now";
  }

  const minutes = Math.floor(deltaSeconds / 60);
  if (minutes < 60) {
    return `${minutes}m ago`;
  }

  const hours = Math.floor(minutes / 60);
  if (hours < 24) {
    return `${hours}h ago`;
  }

  const days = Math.floor(hours / 24);
  if (days < 30) {
    return `${days}d ago`;
  }

  const months = Math.floor(days / 30);
  if (months < 12) {
    return `${months}mo ago`;
  }

  const years = Math.floor(months / 12);
  return `${years}y ago`;
}

export default listCommand;
