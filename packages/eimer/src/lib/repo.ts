import { existsSync, realpathSync } from "node:fs";
import { dirname, join, resolve } from "node:path";

function isRepoRoot(dir: string): boolean {
  return existsSync(join(dir, "packages", "eimer", "package.json"));
}

/**
 * Locates the eimer-cli checkout this binary was built from.
 * Order: $EIMER_HOME, the repo containing the (symlinked) binary in bin/, the repo containing this source file (dev mode).
 */
function resolveRepoDir(): string | null {
  const candidates: string[] = [];

  const fromEnv = (process.env.EIMER_HOME || "").trim();
  if (fromEnv) {
    candidates.push(resolve(fromEnv));
  }

  try {
    candidates.push(dirname(dirname(realpathSync(process.execPath))));
  } catch {}

  candidates.push(resolve(import.meta.dir, "../../../.."));

  return candidates.find((dir) => isRepoRoot(dir)) ?? null;
}

export { resolveRepoDir };
