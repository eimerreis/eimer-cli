import { defineCommand, option } from "@bunli/core";
import { formatRelativeTime, printError, printInfo, renderTable, terminalLink, withSpinner } from "@scripts/ui";
import { z } from "zod";
import {
  buildRunMessage,
  buildRunUrl,
  calculateDuration,
  formatRunStatus,
  getRepoInfo,
  runJson,
  runMatchesRepo,
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
      const repoInfo = await withSpinner("Detecting repository", () => getRepoInfo(), {
        silentFailure: true,
        silentSuccess: true,
      });
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

      const allRuns = await withSpinner(
        "Loading pipeline runs",
        () => runJson<PipelineRun[]>(command),
        { silentFailure: true },
      );
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
        printInfo(`No pipeline runs found for ${scope}.`, flags.all ? undefined : "Try --all to include other repositories.");
        return;
      }

      const rows = runs.map((run) => [
        formatRunStatus(run.status, run.result),
        terminalLink(`#${run.id}`, buildRunUrl(run.id)),
        run.definition?.name || "Unknown pipeline",
        buildRunMessage(run),
        calculateDuration(run.startTime, run.finishTime),
        formatRelativeTime(run.queueTime) || "-",
      ]);

      console.log(renderTable(["State", "Run", "Pipeline", "Message", "Duration", "Queued"], rows, { wordWrap: true }));
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      printError(`Failed to list pipeline runs: ${message}`);
      process.exit(1);
    }
  },
});

export default runsCommand;
