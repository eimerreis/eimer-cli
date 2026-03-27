import { loadConfig } from "@scripts/config";

type WorkItemIdentity = {
  displayName?: string;
  uniqueName?: string;
};

type WorkItemRelation = {
  rel?: string;
  url?: string;
};

type WorkItem = {
  id: number;
  fields: {
    "System.Id"?: number;
    "System.Title"?: string;
    "System.Description"?: string;
    "Custom.Description"?: string;
    "System.State"?: string;
    "System.WorkItemType"?: string;
    "System.AssignedTo"?: WorkItemIdentity | string;
    "System.ChangedDate"?: string;
  };
  relations?: WorkItemRelation[];
};

type AzureContext = {
  baseUrl: string;
  org: string;
  project: string;
};

type Iteration = {
  path?: string;
  attributes?: {
    timeFrame?: string;
  };
};

const DEFAULT_TEAM = "Default Team";
const DEFAULT_AREA_PATH = "Company\\Engineering";

async function getDefaultTeam(): Promise<string> {
  const config = await loadConfig();
  return config.task?.defaultTeam || DEFAULT_TEAM;
}

async function getDefaultAreaPath(): Promise<string> {
  const config = await loadConfig();
  return config.task?.defaultAreaPath || DEFAULT_AREA_PATH;
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

async function loadCurrentIterationByTeam(team?: string): Promise<string | null> {
  const command = [
    "az",
    "boards",
    "iteration",
    "team",
    "list",
    "--timeframe",
    "current",
    "--detect",
    "true",
    "--output",
    "json",
  ];

  if (team) {
    command.push("--team", team);
  }

  try {
    const iterations = await runJson<Iteration[]>(command);
    if (iterations.length === 0) {
      return null;
    }

    const currentIteration = iterations.find(
      (iteration) => (iteration.attributes?.timeFrame || "").toLowerCase() === "current",
    );

    const path = currentIteration?.path || iterations[0].path;
    return path || null;
  } catch {
    return null;
  }
}

function getStateEmoji(state: string): string {
  if (state === "Active") {
    return "[ACTIVE]";
  }

  if (state === "Resolved") {
    return "[RESOLVED]";
  }

  if (state === "Not Started") {
    return "[NOT-STARTED]";
  }

  if (state === "Closed") {
    return "[CLOSED]";
  }

  return `[${state.toUpperCase()}]`;
}

function extractAssignedTo(value: WorkItem["fields"]["System.AssignedTo"]): string {
  if (!value) {
    return "Unassigned";
  }

  if (typeof value === "string") {
    return value;
  }

  return value.displayName || value.uniqueName || "Unassigned";
}

function parseAzureContextFromRemote(remoteUrl: string): AzureContext | null {
  const httpsMatch = remoteUrl.match(/^https:\/\/dev\.azure\.com\/([^/]+)\/([^/]+)\/_git\//i);
  if (httpsMatch) {
    return {
      org: httpsMatch[1],
      project: httpsMatch[2],
      baseUrl: `https://dev.azure.com/${httpsMatch[1]}/${httpsMatch[2]}`,
    };
  }

  const sshMatch = remoteUrl.match(/^(?:ssh:\/\/)?git@ssh\.dev\.azure\.com:v3\/([^/]+)\/([^/]+)\/[^/]+(?:\.git)?$/i);
  if (sshMatch) {
    return {
      org: sshMatch[1],
      project: sshMatch[2],
      baseUrl: `https://dev.azure.com/${sshMatch[1]}/${sshMatch[2]}`,
    };
  }

  const vsSshMatch = remoteUrl.match(/^(?:ssh:\/\/)?[^@]+@([^.]+)\.vs-ssh\.visualstudio\.com:v3\/([^/]+)\/[^/]+(?:\.git)?$/i);
  if (vsSshMatch) {
    return {
      org: vsSshMatch[1],
      project: vsSshMatch[2],
      baseUrl: `https://${vsSshMatch[1]}.visualstudio.com/${vsSshMatch[2]}`,
    };
  }

  return null;
}

async function tryGetAzureContext(): Promise<AzureContext | null> {
  try {
    const remoteUrl = (await runText(["git", "remote", "get-url", "origin"])).trim();
    return parseAzureContextFromRemote(remoteUrl);
  } catch {
    return null;
  }
}

function buildWorkItemUrl(id: number, context: AzureContext | null): string {
  if (!context) {
    return "";
  }

  return `${context.baseUrl}/_workitems/edit/${id}`;
}

function terminalLink(text: string, url: string): string {
  if (!url) {
    return text;
  }

  return `\u001b]8;;${url}\u0007${text}\u001b]8;;\u0007`;
}

function formatRelativeTime(value?: string): string {
  if (!value) {
    return "";
  }

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

function extractParentId(relations?: WorkItemRelation[]): number | null {
  const parentRelation = (relations || []).find((relation) =>
    relation.rel?.toLowerCase().includes("hierarchy-reverse"),
  );

  const relationUrl = parentRelation?.url;
  if (!relationUrl) {
    return null;
  }

  const match = relationUrl.match(/\/workItems\/(\d+)$/i);
  if (!match) {
    return null;
  }

  const id = Number.parseInt(match[1], 10);
  return Number.isFinite(id) ? id : null;
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

export {
  DEFAULT_AREA_PATH,
  DEFAULT_TEAM,
  buildWorkItemUrl,
  extractAssignedTo,
  extractParentId,
  formatRelativeTime,
  getDefaultAreaPath,
  getDefaultTeam,
  getStateEmoji,
  loadCurrentIterationByTeam,
  runJson,
  runText,
  resolveIdArg,
  terminalLink,
  tryGetAzureContext,
};

export type { AzureContext, Iteration, WorkItem };
