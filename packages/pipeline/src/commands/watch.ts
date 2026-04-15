import { defineCommand, option } from "@bunli/core";
import { printError, printInfo, printSuccess, terminalLink, withSpinner } from "@scripts/ui";
import { z } from "zod";
import {
  buildRunMessage,
  buildRunUrl,
  calculateDuration,
  formatRunStatus,
  getRepoInfo,
  resolveIdArg,
  runJson,
  runMatchesRepo,
  type PipelineRun,
} from "./utils";

const watchCommand = defineCommand({
  name: "watch",
  description: "Watch a pipeline run until completion",
  options: {
    id: option(z.coerce.number().int().positive().optional(), {
      short: "i",
      description: "Run ID (defaults to latest run in this repo)",
    }),
    interval: option(z.coerce.number().int().positive().default(15), {
      short: "t",
      description: "Polling interval in seconds",
    }),
    json: option(z.coerce.boolean().default(false), {
      short: "j",
      description: "Print machine-readable JSON on completion",
    }),
  },
  handler: async ({ flags, positional }) => {
    try {
      let runId = resolveIdArg(flags.id, positional);

      if (!runId) {
        runId = await withSpinner("Finding latest pipeline run", () => resolveLatestRepoRunId(), {
          silentFailure: true,
        });
      }

      if (!runId) {
        throw new Error("Missing run ID. Usage: pipeline watch [id]. Try 'pipeline runs' first to pick a run.");
      }

      let lastPrintedStatus = "";
      printInfo(`Watching pipeline run #${runId}.`, `Polling every ${flags.interval}s.`);

      while (true) {
        const run = await runJson<PipelineRun>([
          "az",
          "pipelines",
          "runs",
          "show",
          "--id",
          String(runId),
          "--detect",
          "true",
          "--output",
          "json",
        ]);

        const message = buildRunMessage(run);
        const pipelineName = run.definition?.name || "Unknown pipeline";
        const duration = calculateDuration(run.startTime, run.finishTime);
        const statusLine = `${formatRunStatus(run.status, run.result)} ${terminalLink(`#${run.id}`, buildRunUrl(run.id))} ${pipelineName} | ${message} | ${duration}`;

        if (statusLine !== lastPrintedStatus) {
          console.log(statusLine);
          lastPrintedStatus = statusLine;
        }

        if ((run.status || "").toLowerCase() === "completed") {
          if (flags.json) {
            console.log(JSON.stringify(run, null, 2));
          }

          const result = (run.result || "").toLowerCase();
          if (!flags.json) {
            if (result === "succeeded") {
              printSuccess(`Pipeline run #${run.id} completed successfully.`);
            } else {
              printError(`Pipeline run #${run.id} completed with result '${run.result || "unknown"}'.`);
            }
          }
          process.exit(result === "succeeded" ? 0 : 1);
        }

        await Bun.sleep(flags.interval * 1000);
      }
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      printError(`Failed to watch pipeline run: ${message}`, "Make sure Azure CLI is authenticated and the run belongs to the configured project.");
      process.exit(1);
    }
  },
});

async function resolveLatestRepoRunId(): Promise<number | undefined> {
  const repoInfo = await getRepoInfo();
  const runs = await runJson<PipelineRun[]>([
    "az",
    "pipelines",
    "runs",
    "list",
    "--top",
    "100",
    "--query-order",
    "QueueTimeDesc",
    "--detect",
    "true",
    "--output",
    "json",
  ]);

  const latest = runs.find((run) => runMatchesRepo(run, repoInfo.name));
  return latest?.id;
}

export default watchCommand;
