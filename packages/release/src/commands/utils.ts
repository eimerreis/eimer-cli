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

type AzureContext = {
  organizationUrl: string;
  project: string;
  baseUrl: string;
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

async function getAzureContext(): Promise<AzureContext> {
  const config = await runText(["az", "devops", "configure", "--list"]);
  const organizationUrl = (config.match(/^organization\s*=\s*(.+)$/im)?.[1] || "").trim();
  const project = (config.match(/^project\s*=\s*(.+)$/im)?.[1] || "").trim();

  if (!organizationUrl || !project) {
    throw new Error(
      "Azure DevOps defaults missing. Configure with 'az devops configure -d organization=<url> project=<name>'.",
    );
  }

  return {
    organizationUrl,
    project,
    baseUrl: `${organizationUrl.replace(/\/$/, "")}/${project}`,
  };
}

function buildRunUrl(context: AzureContext, runId: number): string {
  return `${context.baseUrl}/_build/results?buildId=${runId}`;
}

async function resolvePipelineByName(name: string): Promise<PipelineDefinition> {
  const definitions = await runJson<PipelineDefinition[]>([
    "az",
    "pipelines",
    "list",
    "--name",
    name,
    "--top",
    "20",
    "--detect",
    "true",
    "--output",
    "json",
  ]);

  const exact = definitions.find((item) => (item.name || "").toLowerCase() === name.toLowerCase());
  if (!exact) {
    throw new Error(`No pipeline found with name '${name}'.`);
  }

  return exact;
}

async function loadRunById(id: number): Promise<PipelineRun> {
  return runJson<PipelineRun>([
    "az",
    "pipelines",
    "runs",
    "show",
    "--id",
    String(id),
    "--detect",
    "true",
    "--output",
    "json",
  ]);
}

async function loadPipelineRuns(options: {
  pipelineId: number;
  top?: number;
  status?: string;
  result?: string;
}): Promise<PipelineRun[]> {
  const command = [
    "az",
    "pipelines",
    "runs",
    "list",
    "--pipeline-ids",
    String(options.pipelineId),
    "--top",
    String(options.top || 100),
    "--query-order",
    "QueueTimeDesc",
    "--detect",
    "true",
    "--output",
    "json",
  ];

  if (options.status) {
    command.push("--status", options.status);
  }

  if (options.result) {
    command.push("--result", options.result);
  }

  return runJson<PipelineRun[]>(command);
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

export {
  buildRunUrl,
  ensureRunBelongsToPipeline,
  getAzureContext,
  loadPipelineRuns,
  loadRunById,
  normalizeText,
  resolvePipelineByName,
  runJson,
  runText,
};

export type { AzureContext, ChangelogGroups, CommitInfo, ConventionalCommit, PipelineDefinition, PipelineRun };
