import { dirname, join } from "node:path";
import { loadConfig } from "./loader";
import { rcSchema, type ContextDefinition, type EimerConfig, type RcFile } from "./schema";

const RC_FILE_NAME = ".eimerrc";
const CONTEXT_ENV_VAR = "EIMER_CONTEXT";

type ContextSource = "flag" | "env" | "rc" | "global";

type ResolvedContext = ContextDefinition & {
  name: string;
  source: ContextSource;
  /** Path of the .eimerrc file when source is "rc". */
  rcPath?: string;
  /** Normalized organization URL, e.g. https://dev.azure.com/tapioone */
  orgUrl: string;
};

type ContextSelection = {
  name: string;
  source: ContextSource;
  rcPath?: string;
};

type SelectContextInput = {
  flag?: string;
  env?: string;
  rc?: { path: string; context: string } | null;
  defaultContext?: string;
};

/**
 * Pure precedence logic: flag > env > rc > global default.
 */
function selectContext(input: SelectContextInput): ContextSelection | null {
  const flag = input.flag?.trim();
  if (flag) {
    return { name: flag, source: "flag" };
  }

  const env = input.env?.trim();
  if (env) {
    return { name: env, source: "env" };
  }

  if (input.rc?.context) {
    return { name: input.rc.context, source: "rc", rcPath: input.rc.path };
  }

  const fallback = input.defaultContext?.trim();
  if (fallback) {
    return { name: fallback, source: "global" };
  }

  return null;
}

function toOrgUrl(org: string): string {
  const trimmed = org.trim().replace(/\/+$/, "");
  if (/^https?:\/\//i.test(trimmed)) {
    return trimmed;
  }

  return `https://dev.azure.com/${trimmed}`;
}

function describeSource(selection: Pick<ContextSelection, "source" | "rcPath">): string {
  switch (selection.source) {
    case "flag":
      return "--context flag";
    case "env":
      return `${CONTEXT_ENV_VAR} env var`;
    case "rc":
      return selection.rcPath ?? RC_FILE_NAME;
    case "global":
      return "defaultContext in config.json";
  }
}

function materializeContext(selection: ContextSelection, config: EimerConfig): ResolvedContext {
  const definition = config.contexts?.[selection.name];
  if (!definition) {
    const known = Object.keys(config.contexts ?? {});
    const hint = known.length > 0 ? `Known contexts: ${known.join(", ")}.` : "No contexts are defined in config.json.";
    throw new Error(`Unknown context '${selection.name}' (from ${describeSource(selection)}). ${hint}`);
  }

  return {
    ...definition,
    name: selection.name,
    source: selection.source,
    rcPath: selection.rcPath,
    orgUrl: toOrgUrl(definition.org),
  };
}

async function readRcFile(path: string): Promise<RcFile> {
  let raw: unknown;
  try {
    raw = JSON.parse(await Bun.file(path).text()) as unknown;
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    throw new Error(`Failed to parse ${path}: ${message}`);
  }

  const parsed = rcSchema.safeParse(raw);
  if (!parsed.success) {
    throw new Error(`Invalid ${path}: expected { "context": "<name>" }`);
  }

  return parsed.data;
}

/**
 * Walks up from `startDir` to the filesystem root and returns the nearest .eimerrc.
 */
async function findRcFile(startDir: string = process.cwd()): Promise<{ path: string; context: string } | null> {
  let current = startDir;

  while (true) {
    const candidate = join(current, RC_FILE_NAME);
    if (await Bun.file(candidate).exists()) {
      const rc = await readRcFile(candidate);
      return { path: candidate, context: rc.context };
    }

    const parent = dirname(current);
    if (parent === current) {
      return null;
    }

    current = parent;
  }
}

async function writeRcFile(dir: string, context: string): Promise<string> {
  const path = join(dir, RC_FILE_NAME);
  const content = rcSchema.parse({ context });
  await Bun.write(path, `${JSON.stringify(content, null, 2)}\n`);
  return path;
}

type ResolveContextOptions = {
  flag?: string;
  cwd?: string;
  env?: Record<string, string | undefined>;
  config?: EimerConfig;
};

async function resolveContext(options: ResolveContextOptions = {}): Promise<ResolvedContext | null> {
  const config = options.config ?? (await loadConfig());
  const env = options.env ?? process.env;
  const flag = options.flag?.trim();
  const envValue = env[CONTEXT_ENV_VAR]?.trim();

  // Only touch the filesystem when higher-precedence sources are absent.
  const rc = flag || envValue ? null : await findRcFile(options.cwd ?? process.cwd());

  const selection = selectContext({
    flag,
    env: envValue,
    rc,
    defaultContext: config.defaultContext,
  });

  return selection ? materializeContext(selection, config) : null;
}

export {
  CONTEXT_ENV_VAR,
  RC_FILE_NAME,
  describeSource,
  findRcFile,
  materializeContext,
  resolveContext,
  selectContext,
  toOrgUrl,
  writeRcFile,
};
export type { ContextSelection, ContextSource, ResolvedContext };
