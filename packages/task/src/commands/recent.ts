import { defineCommand, option } from "@bunli/core";
import { z } from "zod";
import {
  buildWorkItemUrl,
  extractAssignedTo,
  formatRelativeTime,
  getStateEmoji,
  runJson,
  terminalLink,
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
      const wiql =
        "SELECT [System.Id], [System.Title], [System.State], [System.AssignedTo], [System.ChangedDate], [System.WorkItemType] " +
        "FROM WorkItems WHERE [System.AssignedTo] = @Me AND [System.WorkItemType] = 'Task' ORDER BY [System.ChangedDate] DESC";

      const rows = await runJson<WorkItemQueryRow[]>([
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
        console.log("No recent tasks found.");
        return;
      }

      const context = await tryGetAzureContext();
      for (const item of top) {
        const id = item.fields["System.Id"] || item.id;
        const title = item.fields["System.Title"] || "Untitled";
        const state = item.fields["System.State"] || "Unknown";
        const assignedTo = extractAssignedTo(item.fields["System.AssignedTo"]);
        const changed = formatRelativeTime(item.fields["System.ChangedDate"]);
        const changedSuffix = changed ? ` | ${changed}` : "";
        const url = buildWorkItemUrl(id, context);
        const line = `${getStateEmoji(state)} #${id} ${title} | ${assignedTo}${changedSuffix}`;
        console.log(terminalLink(line, url));
      }
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      console.error(`Failed to list recent tasks: ${message}`);
      process.exit(1);
    }
  },
});

export default recentCommand;
