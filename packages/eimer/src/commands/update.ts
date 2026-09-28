import { defineCommand, option } from "@bunli/core";
import { withHomePath } from "@scripts/config";
import { printError, printInfo, printSuccess, requireTool, runText } from "@scripts/ui";
import { z } from "zod";
import { resolveRepoDir } from "../lib/repo";

const updateCommand = defineCommand({
  name: "update",
  description: "Pull the latest eimer-cli, reinstall dependencies and rebuild",
  options: {
    "no-pull": option(z.coerce.boolean().default(false), {
      argumentKind: "flag",
      description: "Skip git pull (just reinstall and rebuild)",
    }),
    all: option(z.coerce.boolean().default(false), {
      argumentKind: "flag",
      short: "a",
      description: "Also rebuild the standalone pr/pipeline/release/task binaries",
    }),
  },
  handler: async ({ flags }) => {
    try {
      const repoDir = resolveRepoDir();
      if (!repoDir) {
        throw new Error("Could not locate the eimer-cli checkout. Set EIMER_HOME to its path.");
      }

      requireTool("bun");
      requireTool("git");
      printInfo(`Updating ${withHomePath(repoDir)}`);

      if (!flags["no-pull"]) {
        const dirty = (await runText(["git", "-C", repoDir, "status", "--porcelain", "--untracked-files=no"])).trim();
        if (dirty) {
          throw new Error("Working tree has local changes. Commit or stash them first, or rerun with --no-pull.");
        }

        await step(["git", "-C", repoDir, "pull", "--ff-only"], repoDir);
      }

      await step(["bun", "install"], repoDir);
      await step(["bun", "run", flags.all ? "build:all" : "build"], repoDir);

      printSuccess("eimer is up to date. Run `eimer doctor` if anything looks off.");
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      printError(`Update failed: ${message}`);
      process.exit(1);
    }
  },
});

async function step(command: string[], cwd: string): Promise<void> {
  printInfo(`$ ${command.join(" ")}`);
  const process = Bun.spawn({ cmd: command, cwd, stdout: "inherit", stderr: "inherit" });
  const exitCode = await process.exited;
  if (exitCode !== 0) {
    throw new Error(`'${command.join(" ")}' exited with code ${exitCode}.`);
  }
}

export default updateCommand;
