import { defineCommand, option } from "@bunli/core";
import { z } from "zod";
import { getRepoInfo, runJson, runMatchesRepo, runText, type PipelineRun } from "./utils";

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
      let runId = flags.id;
      if (!runId && positional.length > 0) {
        const parsed = Number.parseInt(positional[0], 10);
        runId = Number.isFinite(parsed) ? parsed : undefined;
      }

      if (!runId) {
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
        if (!latest) {
          throw new Error(`No pipeline runs found for repo '${repoInfo.name}'.`);
        }

        runId = latest.id;
      }

      await runText(["az", "pipelines", "runs", "show", "--id", String(runId), "--open", "--detect", "true"]);

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

      console.log(`Opened pipeline run #${runId}.`);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      console.error(`Failed to open pipeline run: ${message}`);
      process.exit(1);
    }
  },
});

export default openCommand;
