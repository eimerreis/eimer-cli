import { defineCommand, option } from "@bunli/core";
import { printError, printSuccess, withSpinner } from "@scripts/ui";
import { z } from "zod";
import { getRepoInfo, resolveIdArg, runJson, runMatchesRepo, runText, type PipelineRun } from "./utils";

const openCommand = defineCommand({
  name: "open",
  description: "Open a pipeline run in browser",
  options: {
    id: option(z.coerce.number().int().positive().optional(), {
      short: "i",
      description: "Run ID (opens latest for repo when omitted)",
    }),
    json: option(z.coerce.boolean().default(false), {
      short: "j",
      description: "Print machine-readable JSON",
    }),
  },
  handler: async ({ flags, positional }) => {
    try {
      let runId = resolveIdArg(flags.id, positional);

      if (!runId) {
        const repoInfo = await withSpinner("Detecting repository", () => getRepoInfo(), {
          silentFailure: true,
          silentSuccess: true,
        });
        const runs = await withSpinner(
          "Loading recent pipeline runs",
          () =>
            runJson<PipelineRun[]>([
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
            ]),
          { silentFailure: true },
        );

        const latest = runs.find((run) => runMatchesRepo(run, repoInfo.name));
        if (!latest) {
          throw new Error(`No pipeline runs found for repo '${repoInfo.name}'. Try 'pipeline runs --all' or pass --id.`);
        }

        runId = latest.id;
      }

      await withSpinner(
        `Opening pipeline run #${runId}`,
        () => runText(["az", "pipelines", "runs", "show", "--id", String(runId), "--open", "--detect", "true"]),
        { silentFailure: true },
      );

      if (flags.json) {
        console.log(
          JSON.stringify(
            {
              opened: true,
              runId,
            },
            null,
            2,
          ),
        );
        return;
      }

      printSuccess(`Opened pipeline run #${runId}.`);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      printError(`Failed to open pipeline run: ${message}`, "Make sure Azure CLI is authenticated and the run exists in the configured project.");
      process.exit(1);
    }
  },
});

export default openCommand;
