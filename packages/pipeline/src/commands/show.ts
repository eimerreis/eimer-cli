import { defineCommand, option } from "@bunli/core";
import { z } from "zod";
import {
  buildRunMessage,
  buildRunUrl,
  calculateDuration,
  formatRelativeTime,
  getRunIndicator,
  resolveIdArg,
  runJson,
  stripBranchPrefix,
  terminalLink,
  type PipelineRun,
} from "./utils";

const showCommand = defineCommand({
  name: "show",
  description: "Show details for a pipeline run",
  options: {
    id: option(z.coerce.number().int().positive().optional(), {
      short: "i",
      description: "Pipeline run ID",
    }),
    json: option(z.coerce.boolean().default(false), {
      short: "j",
      description: "Print machine-readable JSON",
    }),
  },
  handler: async ({ flags, positional, prompt }) => {
    try {
      let runId = resolveIdArg(flags.id, positional);

      if (!runId) {
        const raw = (
          await prompt.text("Pipeline run ID", {
            placeholder: "12345",
            fallbackValue: "",
            validate: (value) => {
              const parsed = Number.parseInt(value.trim(), 10);
              return Number.isFinite(parsed) && parsed > 0 ? true : "Run ID must be a positive integer";
            },
          })
        ).trim();
        runId = Number.parseInt(raw, 10);
      }

      if (!runId || !Number.isFinite(runId) || runId <= 0) {
        throw new Error("Missing run ID. Usage: pipeline show [id]");
      }

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

      if (flags.json) {
        console.log(JSON.stringify(run, null, 2));
        return;
      }

      const indicator = getRunIndicator(run.status, run.result);
      const pipelineName = run.definition?.name || "Unknown pipeline";
      const message = buildRunMessage(run);
      const branch = stripBranchPrefix(run.sourceBranch) || "unknown";
      const requestedBy = run.requestedFor?.displayName || run.requestedFor?.uniqueName || "unknown";
      const queued = formatRelativeTime(run.queueTime) || run.queueTime || "unknown";
      const duration = calculateDuration(run.startTime, run.finishTime);
      const sourceVersion = run.sourceVersion || "unknown";
      const reason = run.reason || "unknown";

      console.log(terminalLink(`${indicator} #${run.id} ${pipelineName}`, buildRunUrl(run.id)));
      console.log(`Message:  ${message}`);
      console.log(`Branch:   ${branch}`);
      console.log(`By:       ${requestedBy}`);
      console.log(`Reason:   ${reason}`);
      console.log(`Queued:   ${queued}`);
      console.log(`Duration: ${duration}`);
      console.log(`Commit:   ${sourceVersion}`);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      console.error(`Failed to show pipeline run: ${message}`);
      process.exit(1);
    }
  },
});

export default showCommand;
