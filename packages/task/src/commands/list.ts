import { defineCommand, option } from "@bunli/core";
import { z } from "zod";
import {
  buildWorkItemUrl,
  extractAssignedTo,
  formatRelativeTime,
  getDefaultTeam,
  getStateEmoji,
  loadCurrentIterationByTeam,
  resolveIdArg,
  runJson,
  terminalLink,
  tryGetAzureContext,
  type WorkItem,
} from "./utils";

type ListItem = {
  id: number;
  title: string;
  state: string;
  type: string;
  assignedTo: string;
  changedDate?: string;
};

const listCommand = defineCommand({
  name: "list",
  description: "List sprint tasks or child tasks under a parent item",
  options: {
    parent: option(z.coerce.number().int().positive().optional(), {
      short: "p",
      description: "Parent work item ID (list child tasks)",
    }),
    team: option(z.string().trim().optional(), {
      short: "t",
      description: "Team name for current sprint lookup",
    }),
    all: option(z.coerce.boolean().default(false), {
      short: "a",
      description: "Include non-active states",
    }),
    top: option(z.coerce.number().int().positive().default(50), {
      short: "n",
      description: "Maximum number of tasks to print",
    }),
    json: option(z.coerce.boolean().default(false), {
      short: "j",
      description: "Print machine-readable JSON",
    }),
  },
  handler: async ({ flags, positional }) => {
    try {
      const parentId = resolveIdArg(flags.parent, positional);
      if (parentId) {
        await listByParent(parentId, flags.all, flags.top, flags.json);
        return;
      }

      const team = flags.team?.trim() || (await getDefaultTeam());
      await listBySprint(team, flags.all, flags.top, flags.json);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      console.error(`Failed to list tasks: ${message}`);
      process.exit(1);
    }
  },
});

async function listBySprint(team: string, includeAllStates: boolean, top: number, asJson: boolean): Promise<void> {
  const iterationPath = await loadCurrentIterationByTeam(team);
  if (!iterationPath) {
    throw new Error(`No active iteration found for team '${team}'.`);
  }

  const stateFilter = includeAllStates ? "" : " AND [System.State] = 'Active'";
  const wiql =
    "SELECT [System.Id], [System.Title], [System.State], [System.AssignedTo], [System.ChangedDate], [System.WorkItemType] " +
    "FROM WorkItems " +
    `WHERE [System.WorkItemType] = 'Task' AND [System.IterationPath] = '${escapeWiqlValue(iterationPath)}'${stateFilter} ` +
    "ORDER BY [System.AssignedTo] ASC, [System.ChangedDate] DESC";

  const rows = await runJson<WorkItem[]>([
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

  const normalized = rows.map(normalizeItem).slice(0, top);

  if (asJson) {
    console.log(
      JSON.stringify(
        {
          mode: "sprint",
          team,
          iterationPath,
          items: normalized,
        },
        null,
        2,
      ),
    );
    return;
  }

  if (normalized.length === 0) {
    console.log(`No ${includeAllStates ? "tasks" : "active tasks"} found for '${iterationPath}'.`);
    return;
  }

  console.log(`Sprint: ${iterationPath} | Team: ${team}`);
  await printItems(normalized);
}

async function listByParent(parentId: number, includeAllStates: boolean, top: number, asJson: boolean): Promise<void> {
  const parent = await loadWorkItem(parentId, true);
  const childIds = extractChildIds(parent.relations);

  const childItems = await Promise.all(childIds.map((id) => loadWorkItem(id, false)));
  const taskChildren = childItems
    .map(normalizeItem)
    .filter((item) => item.type.toLowerCase() === "task")
    .filter((item) => (includeAllStates ? true : item.state.toLowerCase() === "active"))
    .slice(0, top);

  const parentNode = normalizeItem(parent);

  if (asJson) {
    console.log(
      JSON.stringify(
        {
          mode: "parent",
          parent: parentNode,
          items: taskChildren,
        },
        null,
        2,
      ),
    );
    return;
  }

  const context = await tryGetAzureContext();
  const parentLine = `PARENT ${parentNode.type.toUpperCase()} ${getStateEmoji(parentNode.state)} #${parentNode.id} ${parentNode.title} | ${parentNode.assignedTo}`;
  console.log(terminalLink(parentLine, buildWorkItemUrl(parentNode.id, context)));

  if (taskChildren.length === 0) {
    console.log(`No ${includeAllStates ? "task" : "active task"} children found.`);
    return;
  }

  await printItems(taskChildren, "  - ");
}

async function loadWorkItem(id: number, includeRelations: boolean): Promise<WorkItem> {
  const command = ["az", "boards", "work-item", "show", "--id", String(id), "--detect", "true", "--output", "json"];

  if (includeRelations) {
    command.push("--expand", "relations");
  }

  return runJson<WorkItem>(command);
}

function extractChildIds(relations?: WorkItem["relations"]): number[] {
  const ids = new Set<number>();

  for (const relation of relations || []) {
    if (!relation.rel?.toLowerCase().includes("hierarchy-forward") || !relation.url) {
      continue;
    }

    const match = relation.url.match(/\/workItems\/(\d+)$/i);
    if (!match) {
      continue;
    }

    const parsed = Number.parseInt(match[1], 10);
    if (Number.isFinite(parsed)) {
      ids.add(parsed);
    }
  }

  return [...ids];
}

function normalizeItem(item: WorkItem): ListItem {
  return {
    id: item.fields["System.Id"] || item.id,
    title: item.fields["System.Title"] || "Untitled",
    state: item.fields["System.State"] || "Unknown",
    type: item.fields["System.WorkItemType"] || "Work Item",
    assignedTo: extractAssignedTo(item.fields["System.AssignedTo"]),
    changedDate: item.fields["System.ChangedDate"],
  };
}

async function printItems(items: ListItem[], prefix = ""): Promise<void> {
  const context = await tryGetAzureContext();

  for (const item of items) {
    const changed = formatRelativeTime(item.changedDate);
    const changedSuffix = changed ? ` | ${changed}` : "";
    const line = `${prefix}${getStateEmoji(item.state)} #${item.id} ${item.title} | ${item.assignedTo}${changedSuffix}`;
    console.log(terminalLink(line, buildWorkItemUrl(item.id, context)));
  }
}

function escapeWiqlValue(value: string): string {
  return value.replaceAll("'", "''");
}

export default listCommand;
