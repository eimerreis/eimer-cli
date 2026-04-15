import { defineCommand, option } from "@bunli/core";
import { formatRelativeTime, printError, printInfo, renderTable, terminalLink, withSpinner } from "@scripts/ui";
import { z } from "zod";
import {
  buildWorkItemUrl,
  extractAssignedTo,
  getStateEmoji,
  runJson,
  tryGetAzureContext,
} from "./utils";

type WorkItemQueryRow = {
  id: number;
  fields: {
    "System.Id"?: number;
    "System.Title"?: string;
    "System.State"?: string;
    "System.AssignedTo"?: { displayName?: string; uniqueName?: string } | string;
    "System.ChangedDate"?: string;
  };
};

const recentCommand = defineCommand({
  name: "recent",
  description: "List your recent tasks",
  options: {
    all: option(z.coerce.boolean().default(false), {
      short: "a",
      description: "Include closed and non-active states",
    }),
    top: option(z.coerce.number().int().positive().default(10), {
      short: "t",
      description: "Maximum number of tasks to print",
    }),
    json: option(z.coerce.boolean().default(false), {
      short: "j",
      description: "Print machine-readable JSON",
    }),
  },
  handler: async ({ flags }) => {
    try {
      const loadRows = async () => {
        const wiql =
          "SELECT [System.Id], [System.Title], [System.State], [System.AssignedTo], [System.ChangedDate], [System.WorkItemType] " +
          "FROM WorkItems WHERE [System.AssignedTo] = @Me AND [System.WorkItemType] = 'Task' ORDER BY [System.ChangedDate] DESC";

        return runJson<WorkItemQueryRow[]>([
          "az",
          "boards",
          "query",
          "--wiql",
          wiql,
          "--detect",
          "true",
          "--output",
          "json",
        ]);
      };

      const rows = flags.json
        ? await loadRows()
        : await withSpinner("Loading recent tasks", loadRows, { silentFailure: true, silentSuccess: true });

      const filtered = flags.all
        ? rows
        : rows.filter((item) => {
            const state = item.fields["System.State"] || "";
            return state.toLowerCase() === "active";
          });

      const top = filtered.slice(0, flags.top);

      if (flags.json) {
        console.log(JSON.stringify(top, null, 2));
        return;
      }

      if (top.length === 0) {
        printInfo("No recent tasks found.", flags.all ? undefined : "Try --all to include closed and non-active tasks.");
        return;
      }

      const context = await tryGetAzureContext();
      const rowsForTable = top.map((item) => {
        const id = item.fields["System.Id"] || item.id;
        const title = item.fields["System.Title"] || "Untitled";
        const state = item.fields["System.State"] || "Unknown";
        const assignedTo = extractAssignedTo(item.fields["System.AssignedTo"]);
        const changed = formatRelativeTime(item.fields["System.ChangedDate"]);
        const url = buildWorkItemUrl(id, context);

        return [getStateEmoji(state), terminalLink(`#${id}`, url), title, assignedTo, changed || "-"];
      });

      console.log(renderTable(["State", "Task", "Title", "Assigned", "Updated"], rowsForTable, { wordWrap: true }));
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      printError(`Failed to list recent tasks: ${message}`);
      process.exit(1);
    }
  },
});

export default recentCommand;
