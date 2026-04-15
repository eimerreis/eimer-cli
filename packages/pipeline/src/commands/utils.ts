import { colors, symbols } from "@scripts/ui";

type PipelineRun = {
  id: number;
  status?: string;
  result?: string;
  queueTime?: string;
  startTime?: string | null;
  finishTime?: string | null;
  sourceBranch?: string;
  sourceVersion?: string;
  reason?: string;
  triggerInfo?: {
    "ci.message"?: string;
    "pr.number"?: string;
  };
  definition?: {
    id?: number;
    name?: string;
  };
  repository?: {
    name?: string | null;
    type?: string;
    id?: string;
  };
  requestedFor?: {
    displayName?: string;
    uniqueName?: string;
  };
};

type PipelineDefinition = {
  id: number;
  name?: string;
};

type RepoInfo = {
  name: string;
  repositoryFilter: string;
  repositoryType: "tfsgit" | "github";
};

let azureProjectBaseUrlCache: string | null = null;

type RunState = "running" | "queued" | "succeeded" | "failed" | "canceled" | "unknown";

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

function getRunState(status?: string, result?: string): RunState {
  const normalizedStatus = (status || "").toLowerCase();
  const normalizedResult = (result || "").toLowerCase();

  if (normalizedStatus === "inprogress") {
    return "running";
  }

  if (normalizedStatus === "notstarted") {
    return "queued";
  }

  if (normalizedStatus === "completed") {
    if (normalizedResult === "succeeded") {
      return "succeeded";
    }

    if (normalizedResult === "failed") {
      return "failed";
    }

    if (normalizedResult === "canceled") {
      return "canceled";
    }
  }

  return "unknown";
}

function getRunIndicator(status?: string, result?: string): string {
  switch (getRunState(status, result)) {
    case "running":
      return symbols.running;
    case "queued":
      return symbols.queued;
    case "succeeded":
      return symbols.ok;
    case "failed":
      return symbols.fail;
    case "canceled":
      return symbols.warning;
    default:
      return symbols.neutral;
  }
}

function formatRunStatus(status?: string, result?: string): string {
  const normalizedStatus = (status || "").toLowerCase();
  const normalizedResult = (result || "").toLowerCase();

  switch (getRunState(status, result)) {
    case "running":
      return `${symbols.running} ${colors.info("RUNNING")}`;
    case "queued":
      return `${symbols.queued} ${colors.dim("QUEUED")}`;
    case "succeeded":
      return `${symbols.ok} ${colors.success("SUCCEEDED")}`;
    case "failed":
      return `${symbols.fail} ${colors.error("FAILED")}`;
    case "canceled":
      return `${symbols.warning} ${colors.warning("CANCELED")}`;
    default: {
      const label = normalizedResult
        ? normalizedResult.toUpperCase()
        : normalizedStatus
          ? normalizedStatus.toUpperCase()
          : "UNKNOWN";
      return `${symbols.neutral} ${colors.dim(label)}`;
    }
  }
}

function calculateDuration(startTime?: string | null, finishTime?: string | null): string {
  if (!startTime) {
    return "Not started";
  }

  const startMs = Date.parse(startTime);
  if (!Number.isFinite(startMs)) {
    return "Unknown";
  }

  const finishMs = finishTime ? Date.parse(finishTime) : Date.now();
  const safeFinishMs = Number.isFinite(finishMs) ? finishMs : Date.now();
  const elapsedSeconds = Math.max(0, Math.floor((safeFinishMs - startMs) / 1000));
  const minutes = Math.floor(elapsedSeconds / 60);
  const seconds = elapsedSeconds % 60;
  const duration = minutes > 0 ? `${minutes}m ${seconds}s` : `${seconds}s`;

  if (!finishTime) {
    return `${duration} (running)`;
  }

  return duration;
}

function stripBranchPrefix(value?: string): string {
  if (!value) {
    return "";
  }

  return value.startsWith("refs/heads/") ? value.slice("refs/heads/".length) : value;
}

function buildRunMessage(run: PipelineRun): string {
  const fromTrigger = run.triggerInfo?.["ci.message"]?.trim();
  if (fromTrigger) {
    return fromTrigger;
  }

  const prNumber = run.triggerInfo?.["pr.number"]?.trim();
  if (prNumber) {
    return `Build for PR ${prNumber}`;
  }

  const branch = stripBranchPrefix(run.sourceBranch);
  if (branch) {
    return `Build for ${branch}`;
  }

  return "No trigger message";
}

async function getRepoInfo(): Promise<RepoInfo> {
  const remoteUrl = (await runText(["git", "remote", "get-url", "origin"])).trim();

  const httpsAzureMatch = remoteUrl.match(/^https:\/\/dev\.azure\.com\/.+\/_git\/([^/]+?)(?:\.git)?$/i);
  if (httpsAzureMatch) {
    return { name: httpsAzureMatch[1], repositoryFilter: httpsAzureMatch[1], repositoryType: "tfsgit" };
  }

  const sshAzureMatch = remoteUrl.match(/^(?:ssh:\/\/)?git@ssh\.dev\.azure\.com:v3\/[^/]+\/[^/]+\/([^/]+?)(?:\.git)?$/i);
  if (sshAzureMatch) {
    return { name: sshAzureMatch[1], repositoryFilter: sshAzureMatch[1], repositoryType: "tfsgit" };
  }

  const vsSshMatch = remoteUrl.match(
    /^(?:ssh:\/\/)?[^@]+@[^.]+\.vs-ssh\.visualstudio\.com:v3\/[^/]+\/[^/]+\/([^/]+?)(?:\.git)?$/i,
  );
  if (vsSshMatch) {
    return { name: vsSshMatch[1], repositoryFilter: vsSshMatch[1], repositoryType: "tfsgit" };
  }

  const githubMatch = remoteUrl.match(/(?:^https:\/\/github\.com\/|^git@github\.com:)([^/]+)\/([^/]+?)(?:\.git)?$/i);
  if (githubMatch) {
    return {
      name: githubMatch[2],
      repositoryFilter: `${githubMatch[1]}/${githubMatch[2]}`,
      repositoryType: "github",
    };
  }

  throw new Error(`Could not detect repository info from remote URL: ${remoteUrl}`);
}

function runMatchesRepo(run: PipelineRun, repoName: string): boolean {
  const runRepoName = run.repository?.name || "";
  if (runRepoName && runRepoName.toLowerCase() === repoName.toLowerCase()) {
    return true;
  }

  const runRepoId = run.repository?.id || "";
  if (runRepoId) {
    const tail = runRepoId.split("/").pop() || "";
    if (tail.toLowerCase() === repoName.toLowerCase()) {
      return true;
    }
  }

  return false;
}

function getAzureProjectBaseUrl(): string {
  if (azureProjectBaseUrlCache) {
    return azureProjectBaseUrlCache;
  }

  const process = Bun.spawnSync({
    cmd: ["az", "devops", "configure", "--list"],
    stdout: "pipe",
    stderr: "pipe",
  });

  if (process.exitCode === 0) {
    const output = new TextDecoder().decode(process.stdout);
    const organization = (output.match(/^organization\s*=\s*(.+)$/im)?.[1] || "").trim().replace(/\/$/, "");
    const project = (output.match(/^project\s*=\s*(.+)$/im)?.[1] || "").trim();
    if (organization && project) {
      azureProjectBaseUrlCache = `${organization}/${project}`;
      return azureProjectBaseUrlCache;
    }
  }

  azureProjectBaseUrlCache = "https://dev.azure.com/<org>/<project>";
  return azureProjectBaseUrlCache;
}

function buildRunUrl(id: number): string {
  return `${getAzureProjectBaseUrl()}/_build/results?buildId=${id}`;
}

function parseKeyValuePairs(values: string[]): string[] {
  return values
    .map((item) => item.trim())
    .filter((item) => item.length > 0)
    .filter((item) => item.includes("="));
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

function resolveStringArg(flag: string | undefined, positional: string[]): string {
  if (flag?.trim()) {
    return flag.trim();
  }

  return positional.join(" ").trim();
}

export {
  buildRunMessage,
  buildRunUrl,
  calculateDuration,
  formatRunStatus,
  getRepoInfo,
  getRunIndicator,
  parseKeyValuePairs,
  runJson,
  runMatchesRepo,
  runText,
  resolveIdArg,
  resolveStringArg,
  stripBranchPrefix,
};

export type { PipelineDefinition, PipelineRun, RepoInfo };
