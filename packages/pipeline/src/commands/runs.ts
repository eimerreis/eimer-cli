import { defineCommand, option } from "@bunli/core";
import { z } from "zod";
import {
  buildRunMessage,
  buildRunUrl,
  calculateDuration,
  formatRelativeTime,
  getRepoInfo,
  getRunIndicator,
  runJson,
  runMatchesRepo,
  terminalLink,
  type PipelineRun,
} from "./utils";

const runsCommand = defineCommand({
  name: "runs",
  description: "List recent pipeline runs for this repository",
  options: {
    top: option(z.coerce.number().int().positive().default(10), {
      short: "n",
      description: "Maximum number of runs to print",
    }),
    all: option(z.coerce.boolean().default(false), {
      short: "a",
      description: "Include runs from all repositories",
    }),
    branch: option(z.string().trim().optional(), {
      short: "b",
      description: "Filter by branch",
    }),
    result: option(z.string().trim().optional(), {
      short: "r",
      description: "Filter by result (succeeded/failed/canceled)",
    }),
    status: option(z.string().trim().optional(), {
      short: "s",
      description: "Filter by status (inProgress/completed/notStarted)",
    }),
    json: option(z.coerce.boolean().default(false), {
      short: "j",
      description: "Print machine-readable JSON",
    }),
  },
  handler: async ({ flags }) => {
    try {
      const repoInfo = await getRepoInfo();
      const requestedTop = flags.top;
      const fetchTop = flags.all ? requestedTop : Math.min(Math.max(requestedTop * 5, 50), 200);

      const command = [
        "az",
        "pipelines",
        "runs",
        "list",
        "--top",
        String(fetchTop),
        "--query-order",
        "QueueTimeDesc",
        "--detect",
        "true",
        "--output",
        "json",
      ];

      if (flags.branch) {
        command.push("--branch", flags.branch);
      }

      if (flags.status) {
        command.push("--status", flags.status);
      }

      if (flags.result) {
        command.push("--result", flags.result);
      }

      const allRuns = await runJson<PipelineRun[]>(command);
      const filtered = flags.all ? allRuns : allRuns.filter((run) => runMatchesRepo(run, repoInfo.name));
      const runs = filtered.slice(0, requestedTop);

      if (flags.json) {
        console.log(
          JSON.stringify(
            {
              repository: repoInfo,
              count: runs.length,
              items: runs,
            },
            null,
            2,
          ),
        );
        return;
      }

      if (runs.length === 0) {
        const scope = flags.all ? "all repos" : `repo '${repoInfo.name}'`;
        console.log(`No pipeline runs found for ${scope}.`);
        return;
      }

      for (const run of runs) {
        const indicator = getRunIndicator(run.status, run.result);
        const message = buildRunMessage(run);
        const pipelineName = run.definition?.name || "Unknown pipeline";
        const duration = calculateDuration(run.startTime, run.finishTime);
        const when = formatRelativeTime(run.queueTime);
        const whenSuffix = when ? ` | ${when}` : "";
        const line = `${indicator} #${run.id} ${message} | ${pipelineName} | ${duration}${whenSuffix}`;
        console.log(terminalLink(line, buildRunUrl(run.id)));
      }
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      console.error(`Failed to list pipeline runs: ${message}`);
      process.exit(1);
    }
  },
});

export default runsCommand;
