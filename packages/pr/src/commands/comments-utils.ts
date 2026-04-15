import {
  bold,
  createTable,
  dim,
  formatHint,
  formatRelativeTime,
  symbols,
  terminalLink,
} from "@scripts/ui";

type Platform = "github" | "azure-devops";

type GitHubRepo = {
  owner: string;
  name: string;
};

type GitHubPullRequest = {
  number: number;
  title: string;
  html_url?: string;
};

type GitHubReviewComment = {
  id: number;
  body: string;
  path: string;
  line?: number;
  original_line?: number;
  in_reply_to_id?: number;
  created_at?: string;
  user?: {
    login?: string;
  };
};

type AzurePullRequest = {
  pullRequestId: number;
  title: string;
  sourceRefName?: string;
  url?: string;
  repository?: {
    id?: string;
    name?: string;
  };
  creationDate?: string;
};

type AzureComment = {
  id: number;
  parentCommentId: number;
  content?: string;
  commentType?: string;
  isDeleted?: boolean;
  publishedDate?: string;
  author?: {
    displayName?: string;
  };
};

type AzureThread = {
  id: number;
  status?: string;
  isDeleted?: boolean;
  lastUpdatedDate?: string;
  threadContext?: {
    filePath?: string;
    rightFileStart?: {
      line?: number;
    };
  };
  comments?: AzureComment[];
};

type NormalizedComment = {
  id: number;
  parentCommentId?: number;
  author: string;
  body: string;
  createdAt?: string;
};

type NormalizedThread = {
  id: number;
  status: string;
  filePath?: string;
  line?: number;
  location: string;
  comments: NormalizedComment[];
  updatedAt?: string;
};

type NormalizedResult = {
  platform: Platform;
  pr: {
    id: number;
    title: string;
    branch?: string;
    repository?: string;
    url?: string;
  };
  threads: NormalizedThread[];
};

const AZURE_DEVOPS_RESOURCE = "499b84ac-1321-427f-aa17-267ca6975798";

function detectPlatform(remoteUrl: string): Platform {
  const normalized = remoteUrl.toLowerCase();
  if (normalized.includes("github.com")) {
    return "github";
  }

  if (
    normalized.includes("dev.azure.com") ||
    normalized.includes("ssh.dev.azure.com") ||
    normalized.includes("vs-ssh.visualstudio.com")
  ) {
    return "azure-devops";
  }

  throw new Error(`Unsupported remote URL: ${remoteUrl}`);
}

function mapAzureStatus(status?: string): string {
  switch ((status || "unknown").toLowerCase()) {
    case "wontfix":
      return "won't fix";
    case "bydesign":
      return "by design";
    default:
      return (status || "unknown").toLowerCase();
  }
}

function buildLocation(filePath?: string, line?: number): string {
  if (!filePath) {
    return "(general)";
  }

  if (!line) {
    return filePath;
  }

  return `${filePath}:${line}`;
}

function normalizeNewlines(value: string): string {
  return value.replace(/\r\n/g, "\n").trim();
}

function resolveIdArg(flag: number | undefined, positional: string[]): number | undefined {
  if (flag) {
    return flag;
  }

  const raw = positional[0]?.trim();
  if (!raw) {
    return undefined;
  }

  const parsed = Number.parseInt(raw, 10);
  if (Number.isFinite(parsed) && parsed > 0) {
    return parsed;
  }

  return undefined;
}

function printHumanReadable(result: NormalizedResult): void {
  const platformLabel = result.platform === "github" ? "GitHub" : "Azure DevOps";
  console.log(`${bold(`PR #${result.pr.id}`)} ${result.pr.title}`);
  console.log(dim(`${platformLabel}${result.pr.repository ? ` | ${result.pr.repository}` : ""}`));

  if (result.threads.length === 0) {
    console.log(`${symbols.info} No review comment threads found.`);
    console.log(formatHint("Try `pr show` to inspect approval and merge-readiness details."));
    return;
  }

  for (const thread of result.threads) {
    console.log("");
    const summary = createTable(["Location", "Status", "Updated"], [
      [thread.location, thread.status, thread.updatedAt ? formatRelativeTime(thread.updatedAt) : "-"],
    ], { compact: true });
    console.log(summary.toString());

    for (const comment of thread.comments) {
      const lines = normalizeNewlines(comment.body).split("\n").filter(Boolean);
      const authoredAt = comment.createdAt ? dim(` (${formatRelativeTime(comment.createdAt)})`) : "";
      if (lines.length === 0) {
        console.log(`  ${symbols.arrow} ${bold(comment.author)}${authoredAt}`);
        continue;
      }

      console.log(`  ${symbols.arrow} ${bold(comment.author)}${authoredAt}`);
      console.log(`    ${terminalLink(lines[0], result.pr.url || "")}`);
      for (const line of lines.slice(1)) {
        console.log(`    ${line}`);
      }
    }
  }
}

async function runText(command: string[]): Promise<string> {
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

  if (exitCode !== 0) {
    throw new Error(`Command failed (${command.join(" ")}): ${stderr.trim() || stdout.trim()}`);
  }

  return stdout;
}

async function runJson<T>(command: string[]): Promise<T> {
  const raw = await runText(command);

  try {
    return JSON.parse(raw) as T;
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    throw new Error(`Failed to parse JSON from '${command.join(" ")}': ${message}`);
  }
}

export {
  AZURE_DEVOPS_RESOURCE,
  buildLocation,
  detectPlatform,
  mapAzureStatus,
  printHumanReadable,
  resolveIdArg,
  runJson,
  runText,
};

export type {
  AzurePullRequest,
  AzureThread,
  GitHubPullRequest,
  GitHubRepo,
  GitHubReviewComment,
  NormalizedResult,
  NormalizedThread,
  Platform,
};
