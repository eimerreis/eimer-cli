import { getAzureClient } from "./utils";

type BuildTimelineRecord = {
  type?: string;
  identifier?: string;
  name?: string;
  result?: string;
  state?: string;
};

type BuildTimeline = {
  records?: BuildTimelineRecord[];
};

type PipelineStageRecord = {
  identifier: string;
  name: string;
  result: string;
  state: string;
};

type ProdStageCache = Map<string, boolean>;

async function loadPipelineTimeline(baseUrl: string, runId: number): Promise<BuildTimeline> {
  const { client, context } = await getAzureClient();
  if (context.baseUrl !== baseUrl) {
    throw new Error(
      `Azure DevOps context mismatch. Expected '${baseUrl}', resolved '${context.baseUrl}'. Set SYSTEM_COLLECTIONURI and SYSTEM_TEAMPROJECT to align context.`,
    );
  }

  return client.getJson<BuildTimeline>(`/_apis/build/builds/${runId}/timeline`);
}

async function listPipelineStages(baseUrl: string, runId: number): Promise<PipelineStageRecord[]> {
  const timeline = await loadPipelineTimeline(baseUrl, runId);
  const records = timeline.records || [];

  return records
    .filter((record) => normalizeText(record.type || "") === "stage")
    .map((record) => ({
      identifier: (record.identifier || "").trim(),
      name: (record.name || "").trim(),
      result: normalizeText(record.result || ""),
      state: normalizeText(record.state || ""),
    }));
}

async function hasSuccessfulProdStage(options: {
  baseUrl: string;
  runId: number;
  cache: ProdStageCache;
  prodStageName?: string;
}): Promise<boolean> {
  const normalizedConfiguredProdStage = normalizeText(options.prodStageName || "");
  const cacheKey = `${options.runId}::${normalizedConfiguredProdStage || "auto"}`;
  const cached = options.cache.get(cacheKey);
  if (typeof cached === "boolean") {
    return cached;
  }

  const stages = await listPipelineStages(options.baseUrl, options.runId);
  const hasProd = stages.some((stage) => {
    if (!isSuccessfulStage(stage)) {
      return false;
    }

    if (normalizedConfiguredProdStage) {
      return matchesConfiguredStage(stage, normalizedConfiguredProdStage);
    }

    return looksLikeProdStage(stage);
  });

  options.cache.set(cacheKey, hasProd);
  return hasProd;
}

function detectProdStageName(stages: PipelineStageRecord[]): string | null {
  const successful = stages.filter(isSuccessfulStage);
  const candidate = successful.find(looksLikeProdStage) || stages.find(looksLikeProdStage);
  if (!candidate) {
    return null;
  }

  return candidate.identifier || candidate.name || null;
}

function isSuccessfulStage(stage: PipelineStageRecord): boolean {
  return stage.state === "completed" && stage.result === "succeeded";
}

function matchesConfiguredStage(stage: PipelineStageRecord, configuredStageName: string): boolean {
  const configuredCanonical = canonicalStageName(configuredStageName);
  const stageIdentifierCanonical = canonicalStageName(stage.identifier);
  const stageNameCanonical = canonicalStageName(stage.name);

  return configuredCanonical === stageIdentifierCanonical || configuredCanonical === stageNameCanonical;
}

function looksLikeProdStage(stage: PipelineStageRecord): boolean {
  const identifier = normalizeText(stage.identifier);
  const name = normalizeText(stage.name);

  if (!identifier && !name) {
    return false;
  }

  const identifierParts = splitStageParts(identifier);
  const nameParts = splitStageParts(name);
  const allParts = new Set([...identifierParts, ...nameParts]);
  const hasDeploy = allParts.has("deploy") || allParts.has("deployment");
  const hasProd = allParts.has("prod") || allParts.has("production");
  const hasRelease = allParts.has("release");
  const hasLive = allParts.has("live");

  if (identifier === "deploy_prod") {
    return true;
  }

  if (hasProd) {
    return true;
  }

  if (hasLive && hasDeploy) {
    return true;
  }

  if (hasRelease) {
    return true;
  }

  return false;
}

function splitStageParts(value: string): string[] {
  return value.split(/[^a-z0-9]+/).filter(Boolean);
}

function canonicalStageName(value: string): string {
  return normalizeText(value).replaceAll(/[^a-z0-9]/g, "");
}

function normalizeText(value: string): string {
  return value.trim().toLowerCase();
}

export { detectProdStageName, hasSuccessfulProdStage, listPipelineStages };
export type { PipelineStageRecord, ProdStageCache };
