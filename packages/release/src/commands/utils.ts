import { createAzureDevOpsClient, type AzureContext, type AzureDevOpsClient } from "../azure-client";

type PipelineDefinition = {
  id: number;
  name?: string;
};

type PipelineRun = {
  id: number;
  status?: string;
  result?: string;
  queueTime?: string;
  startTime?: string | null;
  finishTime?: string | null;
  sourceBranch?: string;
  sourceVersion?: string;
  definition?: {
    id?: number;
    name?: string;
  };
  repository?: {
    id?: string;
    name?: string;
    type?: string;
  };
};

type CommitInfo = {
  hash: string;
  shortHash: string;
  subject: string;
  conventional: ConventionalCommit;
  prNumber?: number;
  prUrl?: string;
};

type ConventionalCommit = {
  type: string;
  scope: string;
  description: string;
  raw: string;
  isConventional: boolean;
};

type ChangelogGroups = {
  features: CommitInfo[];
  fixes: CommitInfo[];
  chores: CommitInfo[];
  docs: CommitInfo[];
  refactors: CommitInfo[];
  perf: CommitInfo[];
  tests: CommitInfo[];
  buildCi: CommitInfo[];
  other: CommitInfo[];
};

type PipelineDefinitionListResponse = {
  value?: PipelineDefinition[];
};

type PipelineRunListResponse = {
  value?: PipelineRun[];
};

let azureClientPromise: Promise<{ context: AzureContext; client: AzureDevOpsClient }> | undefined;

async function getAzureClient(): Promise<{ context: AzureContext; client: AzureDevOpsClient }> {
  if (!azureClientPromise) {
    azureClientPromise = createAzureDevOpsClient();
  }

  return azureClientPromise;
}

async function getAzureContext(): Promise<AzureContext> {
  const { context } = await getAzureClient();
  return context;
}

function buildRunUrl(context: AzureContext, runId: number): string {
  return `${context.baseUrl}/_build/results?buildId=${runId}`;
}

async function resolvePipelineByName(name: string): Promise<PipelineDefinition> {
  const { client } = await getAzureClient();
  const response = await client.getJson<PipelineDefinitionListResponse>("/_apis/build/definitions", {
    name,
    "$top": 20,
  });
  const definitions = response.value || [];

  const exact = definitions.find((item) => (item.name || "").toLowerCase() === name.toLowerCase());
  if (!exact) {
    throw new Error(`No pipeline found with name '${name}'.`);
  }

  return exact;
}

async function loadRunById(id: number): Promise<PipelineRun> {
  const { client } = await getAzureClient();
  return client.getJson<PipelineRun>(`/_apis/build/builds/${id}`);
}

async function loadPipelineRuns(options: {
  pipelineId: number;
  top?: number;
  status?: string;
  result?: string;
  branch?: string;
}): Promise<PipelineRun[]> {
  const { client } = await getAzureClient();
  const response = await client.getJson<PipelineRunListResponse>("/_apis/build/builds", {
    definitions: options.pipelineId,
    "$top": options.top || 100,
    queryOrder: "queueTimeDescending",
    statusFilter: options.status,
    resultFilter: options.result,
    branchName: options.branch,
  });

  return response.value || [];
}

function ensureRunBelongsToPipeline(run: PipelineRun, pipelineId: number): void {
  const runPipelineId = run.definition?.id;
  if (runPipelineId && runPipelineId !== pipelineId) {
    throw new Error(`Run #${run.id} does not belong to pipeline ID ${pipelineId}.`);
  }
}

function normalizeText(value: string): string {
  return value.trim().toLowerCase();
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
  buildRunUrl,
  ensureRunBelongsToPipeline,
  getAzureClient,
  getAzureContext,
  loadPipelineRuns,
  loadRunById,
  normalizeText,
  resolveIdArg,
  resolveStringArg,
  resolvePipelineByName,
};

export type { AzureContext, ChangelogGroups, CommitInfo, ConventionalCommit, PipelineDefinition, PipelineRun };
