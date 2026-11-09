import { mkdir } from "node:fs/promises";
import { z } from "zod";

const requiredString = z.string().trim().min(1);

const areaConfigSchema = z.object({
  includeScopes: z.array(requiredString),
  excludeScopes: z.array(requiredString),
  excludeKeywords: z.array(requiredString),
});

const configSchema = z
  .object({
    teams: z
      .object({
        webhookUrl: requiredString.optional(),
        channels: z.record(requiredString, requiredString).optional(),
      })
      .optional(),
    release: z
      .object({
        defaultPipeline: requiredString.optional(),
        prodStageName: requiredString.optional(),
        defaultBranch: requiredString.optional(),
        // Remembered prod stage per pipeline name (written by the eimer CLI).
        prodStages: z.record(requiredString, requiredString).optional(),
      })
      .optional(),
    areas: z.record(requiredString, areaConfigSchema).optional(),
  })
  .strict();

type ReleaseAreaConfig = z.infer<typeof areaConfigSchema>;
type ReleaseConfig = z.infer<typeof configSchema>;

function getConfigDir(): string {
  const homeDir = process.env.HOME || Bun.env.HOME;
  if (!homeDir) {
    throw new Error("Could not resolve HOME directory for release config.");
  }

  return `${homeDir}/.config/tapio-release`;
}

function getConfigPath(): string {
  return `${getConfigDir()}/config.json`;
}

async function loadConfig(): Promise<ReleaseConfig> {
  const path = getConfigPath();
  const file = Bun.file(path);
  if (!(await file.exists())) {
    return {};
  }

  let raw: unknown;
  try {
    raw = JSON.parse(await file.text()) as unknown;
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    throw new Error(`Failed to parse config at ${path}: ${message}`);
  }

  try {
    return configSchema.parse(raw);
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    throw new Error(`Invalid config at ${path}: ${message}`);
  }
}

async function saveConfig(nextConfig: ReleaseConfig): Promise<ReleaseConfig> {
  const parsed = configSchema.parse(nextConfig);
  await mkdir(getConfigDir(), { recursive: true });
  await Bun.write(getConfigPath(), `${JSON.stringify(parsed, null, 2)}\n`);
  return parsed;
}

function resolveChannels(config: ReleaseConfig): Record<string, string> {
  const channels: Record<string, string> = {};

  const defaultWebhook = (config.teams?.webhookUrl || "").trim();
  if (defaultWebhook) {
    channels.default = defaultWebhook;
  }

  const configured = config.teams?.channels || {};
  for (const [name, url] of Object.entries(configured)) {
    const normalizedName = name.trim();
    const normalizedUrl = url.trim();
    if (!normalizedName || !normalizedUrl) {
      continue;
    }

    channels[normalizedName] = normalizedUrl;
  }

  return channels;
}

function withHomePath(path: string): string {
  const homeDir = process.env.HOME || Bun.env.HOME;
  return homeDir ? path.replace(homeDir, "~") : path;
}

/** Remembered prod stage for the first matching pipeline name (case-insensitive). */
function prodStageForPipeline(config: ReleaseConfig, ...pipelineNames: Array<string | undefined>): string {
  const stages = Object.entries(config.release?.prodStages || {});
  for (const candidate of pipelineNames) {
    const wanted = (candidate || "").trim().toLowerCase();
    if (!wanted) continue;
    const match = stages.find(([name]) => name.trim().toLowerCase() === wanted);
    if (match) return match[1].trim();
  }
  return "";
}

export { getConfigPath, loadConfig, prodStageForPipeline, resolveChannels, saveConfig, withHomePath };
export type { ReleaseAreaConfig, ReleaseConfig };
