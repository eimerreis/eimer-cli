import { defineCommand, option } from "@bunli/core";
import { getConfigPath, loadConfig, withHomePath } from "@scripts/config";
import { getConfigPath as getReleaseConfigPath, loadConfig as loadReleaseConfig } from "@scripts/release/config";
import { bold, dim, knownTools, runRaw, symbols } from "@scripts/ui";
import { existsSync, realpathSync } from "node:fs";
import { z } from "zod";
import { resolveRepoDir } from "../lib/repo";
import { version } from "../lib/version";

type Status = "ok" | "warn" | "fail";

type Check = {
  label: string;
  status: Status;
  detail: string;
  hint?: string;
};

const MIN_BUN_VERSION = "1.3.11";

const doctorCommand = defineCommand({
  name: "doctor",
  description: "Check tools, logins, config and PATH setup",
  options: {
    json: option(z.coerce.boolean().default(false), {
      argumentKind: "flag",
      short: "j",
      description: "Print machine-readable JSON",
    }),
  },
  handler: async ({ flags }) => {
    const checks = [
      ...(await checkTools()),
      ...(await checkAzure()),
      await checkGitHub(),
      await checkEimerConfig(),
      await checkReleaseConfig(),
      checkClipboard(),
      checkInstall(),
    ];

    if (flags.json) {
      console.log(JSON.stringify({ version, checks }, null, 2));
    } else {
      printChecks(checks);
    }

    if (checks.some((check) => check.status === "fail")) {
      process.exit(1);
    }
  },
});

async function checkTools(): Promise<Check[]> {
  const checks: Check[] = [];

  const bunPath = Bun.which("bun");
  if (!bunPath) {
    checks.push({
      label: "bun",
      status: "warn",
      detail: "not on PATH (only needed to build/update eimer)",
      hint: "curl -fsSL https://bun.sh/install | bash",
    });
  } else {
    const { stdout } = await runRaw(["bun", "--version"]);
    const bunVersion = stdout.trim();
    const outdated = compareVersions(bunVersion, MIN_BUN_VERSION) < 0;
    checks.push({
      label: "bun",
      status: outdated ? "warn" : "ok",
      detail: bunVersion,
      hint: outdated ? `Needs >= ${MIN_BUN_VERSION}: bun upgrade` : undefined,
    });
  }

  checks.push(checkBinary("git"));
  return checks;
}

function checkBinary(command: string, required = true): Check {
  const tool = knownTools[command];
  const path = Bun.which(command);
  if (path) {
    return { label: command, status: "ok", detail: withHomePath(path) };
  }

  return {
    label: command,
    status: required ? "fail" : "warn",
    detail: `not installed${tool ? ` (needed for ${tool.purpose})` : ""}`,
    hint: tool?.install,
  };
}

async function checkAzure(): Promise<Check[]> {
  const az = checkBinary("az");
  if (az.status !== "ok") {
    return [az];
  }

  const checks: Check[] = [az];

  const extension = await runRaw(["az", "extension", "show", "--name", "azure-devops", "--query", "version", "--output", "tsv"]);
  checks.push(
    extension.exitCode === 0
      ? { label: "az devops extension", status: "ok", detail: extension.stdout.trim() }
      : { label: "az devops extension", status: "fail", detail: "not installed", hint: "az extension add --name azure-devops" },
  );

  const account = await runRaw(["az", "account", "show", "--query", "user.name", "--output", "tsv"]);
  checks.push(
    account.exitCode === 0
      ? { label: "az login", status: "ok", detail: account.stdout.trim() }
      : { label: "az login", status: "fail", detail: "not logged in", hint: "az login" },
  );

  const defaults = await runRaw(["az", "devops", "configure", "--list"]);
  const organization = defaults.stdout.match(/^organization\s*=\s*(.*)$/m)?.[1]?.trim();
  const project = defaults.stdout.match(/^project\s*=\s*(.*)$/m)?.[1]?.trim();
  checks.push(
    organization && project
      ? { label: "az devops defaults", status: "ok", detail: `${organization} / ${project}` }
      : {
          label: "az devops defaults",
          status: "warn",
          detail: "organization/project not set (commands fall back to git remote detection)",
          hint: "az devops configure --defaults organization=https://dev.azure.com/<org> project=<project>",
        },
  );

  return checks;
}

async function checkGitHub(): Promise<Check> {
  const gh = checkBinary("gh", false);
  if (gh.status !== "ok") {
    return gh;
  }

  const auth = await runRaw(["gh", "auth", "status"]);
  if (auth.exitCode !== 0) {
    return { label: "gh", status: "warn", detail: "installed but not logged in", hint: "gh auth login" };
  }

  return { label: "gh", status: "ok", detail: "logged in" };
}

async function checkEimerConfig(): Promise<Check> {
  const path = withHomePath(getConfigPath());
  try {
    const config = await loadConfig();
    const configured = Boolean(config.task?.defaultTeam || config.task?.defaultAreaPath);
    return configured
      ? { label: "eimer config", status: "ok", detail: path }
      : { label: "eimer config", status: "warn", detail: `no task defaults in ${path}`, hint: "eimer configure" };
  } catch (error) {
    return { label: "eimer config", status: "fail", detail: errorMessage(error), hint: "eimer configure --reset" };
  }
}

async function checkReleaseConfig(): Promise<Check> {
  const path = withHomePath(getReleaseConfigPath());
  try {
    const config = await loadReleaseConfig();
    return config.release?.defaultPipeline
      ? { label: "release config", status: "ok", detail: path }
      : { label: "release config", status: "warn", detail: `no default pipeline in ${path}`, hint: "eimer release configure" };
  } catch (error) {
    return { label: "release config", status: "fail", detail: errorMessage(error), hint: `Fix or delete ${path}` };
  }
}

function checkClipboard(): Check {
  if (process.platform === "darwin" || process.platform === "win32") {
    return { label: "clipboard", status: "ok", detail: "built-in" };
  }

  const tool = ["wl-copy", "xclip"].find((candidate) => Bun.which(candidate));
  return tool
    ? { label: "clipboard", status: "ok", detail: tool }
    : { label: "clipboard", status: "warn", detail: "no wl-copy or xclip (pr copy won't work)", hint: "install wl-clipboard or xclip" };
}

function checkInstall(): Check {
  const repoDir = resolveRepoDir();
  const onPath = Bun.which("eimer");
  const repoHint = repoDir ? `cd ${withHomePath(repoDir)} && bun run setup` : "run `bun run setup` in your eimer-cli checkout";

  if (!onPath) {
    return { label: "eimer on PATH", status: "warn", detail: "not found", hint: repoHint };
  }

  let target = onPath;
  try {
    target = realpathSync(onPath);
  } catch {}

  const fromRepo = repoDir ? target.startsWith(repoDir) : false;
  const detail = `${withHomePath(onPath)}${target !== onPath ? ` -> ${withHomePath(target)}` : ""}`;
  if (repoDir && !fromRepo && existsSync(`${repoDir}/bin/eimer`)) {
    return { label: "eimer on PATH", status: "warn", detail: `${detail} (not this checkout)`, hint: repoHint };
  }

  return { label: "eimer on PATH", status: "ok", detail };
}

function printChecks(checks: Check[]): void {
  console.log(bold(`eimer ${version} doctor`));
  console.log("");

  const width = Math.max(...checks.map((check) => check.label.length));
  for (const check of checks) {
    const symbol = check.status === "ok" ? symbols.ok : check.status === "warn" ? symbols.warning : symbols.fail;
    console.log(`${symbol} ${check.label.padEnd(width)}  ${check.detail}`);
    if (check.hint && check.status !== "ok") {
      console.log(`  ${" ".repeat(width)}  ${dim(`-> ${check.hint}`)}`);
    }
  }

  const failures = checks.filter((check) => check.status === "fail").length;
  const warnings = checks.filter((check) => check.status === "warn").length;
  console.log("");
  console.log(failures || warnings ? `${failures} problem(s), ${warnings} warning(s).` : "Everything looks good.");
}

function compareVersions(left: string, right: string): number {
  const leftParts = left.split(".").map((part) => Number.parseInt(part, 10) || 0);
  const rightParts = right.split(".").map((part) => Number.parseInt(part, 10) || 0);
  for (let index = 0; index < Math.max(leftParts.length, rightParts.length); index += 1) {
    const difference = (leftParts[index] ?? 0) - (rightParts[index] ?? 0);
    if (difference !== 0) {
      return difference;
    }
  }

  return 0;
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

export default doctorCommand;
