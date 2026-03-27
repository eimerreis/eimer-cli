import { z } from "zod";

const stringValue = z.string().trim().min(1);

const areaConfigSchema = z.object({
  includeScopes: z.array(stringValue),
  excludeScopes: z.array(stringValue),
  excludeKeywords: z.array(stringValue),
});

const configSchema = z
  .object({
    teams: z
      .object({
        webhookUrl: stringValue.optional(),
      })
      .optional(),
    release: z
      .object({
        defaultPipeline: stringValue.optional(),
      })
      .optional(),
    areas: z.record(stringValue, areaConfigSchema).optional(),
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

export { getConfigPath, loadConfig };
export type { ReleaseAreaConfig, ReleaseConfig };
