import { mkdir, rm } from "node:fs/promises";
import { configSchema, type EimerConfig } from "./schema";

function getConfigDir(): string {
  const homeDir = process.env.HOME || Bun.env.HOME;
  if (!homeDir) {
    throw new Error("Could not resolve HOME directory for eimer config.");
  }

  return `${homeDir}/.config/eimer`;
}

function getConfigPath(): string {
  return `${getConfigDir()}/config.json`;
}

async function loadConfig(): Promise<EimerConfig> {
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

async function saveConfig(nextConfig: EimerConfig): Promise<EimerConfig> {
  const parsed = configSchema.parse(nextConfig);
  await mkdir(getConfigDir(), { recursive: true });
  await Bun.write(getConfigPath(), `${JSON.stringify(parsed, null, 2)}\n`);
  return parsed;
}

async function updateConfig(updater: (current: EimerConfig) => EimerConfig): Promise<EimerConfig> {
  const current = await loadConfig();
  return saveConfig(updater(current));
}

async function deleteConfig(): Promise<void> {
  const path = getConfigPath();
  const file = Bun.file(path);
  if (await file.exists()) {
    await rm(path);
  }
}

function withHomePath(path: string): string {
  const homeDir = process.env.HOME || Bun.env.HOME;
  return homeDir ? path.replace(homeDir, "~") : path;
}

export { deleteConfig, getConfigPath, loadConfig, saveConfig, updateConfig, withHomePath };
export type { EimerConfig };
