import { option } from "@bunli/core";
import { formatRelativeTime, printError, renderTable, terminalLink, withSpinner } from "@scripts/ui";
import { z } from "zod";
import {
  azScopeArgs,
  buildWorkItemUrl,
  contextOption,
  extractAssignedTo,
  getActiveContext,
  getDefaultTeam,
  loadCurrentIterationByTeam,
  runJson,
  tryGetAzureContext,
  getStateEmoji,
  initContext,
  type WorkItem,
} from "../utils";

type ListItem = {
  id: number;
  title: string;
  state: string;
  type: string;
  assignedTo: string;
  changedDate?: string;
};

/**
 * State names meaning "not started yet" across the process templates we use
 * (Agile: New, Scrum: To Do, Basic/custom: Not Started, CMMI: Proposed).
 */
const NOT_STARTED_STATES = ["New", "To Do", "Not Started", "Proposed"] as const;

const LIST_FIELDS =
  "[System.Id], [System.Title], [System.State], [System.AssignedTo], [System.ChangedDate], [System.WorkItemType]";

const teamOption = option(z.string().trim().optional(), {
  short: "t",
  description: "Team name for current sprint lookup (defaults to the context's team)",
});

const topOption = option(z.coerce.number().int().positive().default(50), {
  short: "n",
  description: "Maximum number of tasks to print",
});

const jsonOption = option(z.coerce.boolean().default(false), {
  argumentKind: "flag",
  short: "j",
  description: "Print machine-readable JSON",
});

const sharedListOptions = {
  top: topOption,
  context: contextOption,
  json: jsonOption,
};

/**
 * Resolves the context, then runs the action (with a spinner unless --json).
 * Any failure is printed and exits with code 1.
 */
async function runListAction(
  label: string,
  flags: { json: boolean; context?: string },
  action: () => Promise<void>,
): Promise<void> {
  try {
    await initContext(flags.context, { quiet: flags.json });

    if (flags.json) {
      await action();
    } else {
      await withSpinner(label, action, { silentFailure: true, silentSuccess: true });
    }
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    printError(`Failed to list tasks: ${message}`);
    process.exit(1);
  }
}

async function resolveSprint(teamOverride: string | undefined): Promise<{ team: string; iterationPath: string }> {
  const team = teamOverride?.trim() || (await getDefaultTeam());
  const iterationPath = await loadCurrentIterationByTeam(team);
  if (!iterationPath) {
    throw new Error(`No active iteration found for team '${team}'. Pass --team or set a team on the context.`);
  }

  return { team, iterationPath };
}

async function queryTasks(whereClause: string, orderBy: string): Promise<ListItem[]> {
  const wiql = `SELECT ${LIST_FIELDS} FROM WorkItems WHERE [System.WorkItemType] = 'Task' AND ${whereClause} ORDER BY ${orderBy}`;
  const rows = await runJson<WorkItem[]>(["az", "boards", "query", "--wiql", wiql, ...azScopeArgs(), "--output", "json"]);
  return rows.map(normalizeItem);
}

async function loadWorkItem(id: number, includeRelations: boolean): Promise<WorkItem> {
  const command = [
    "az",
    "boards",
    "work-item",
    "show",
    "--id",
    String(id),
    ...azScopeArgs({ project: false }),
    "--output",
    "json",
  ];

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

async function printItems(items: ListItem[]): Promise<void> {
  const context = await tryGetAzureContext();
  const rows = items.map((item) => [
    getStateEmoji(item.state),
    terminalLink(`#${item.id}`, buildWorkItemUrl(item.id, context)),
    item.title,
    item.assignedTo,
    item.changedDate ? formatRelativeTime(item.changedDate) : "-",
  ]);

  console.log(renderTable(["State", "Task", "Title", "Assigned", "Updated"], rows, { wordWrap: true }));
}

function printJson(payload: Record<string, unknown>): void {
  console.log(JSON.stringify({ context: getActiveContext()?.name ?? null, ...payload }, null, 2));
}

/** Second line for list headers, e.g. "Team: Core · Context: tapio". */
function describeScope(team?: string): string {
  const parts: string[] = [];
  if (team) {
    parts.push(`Team: ${team}`);
  }

  const context = getActiveContext();
  if (context) {
    parts.push(`Context: ${context.name}`);
  }

  return parts.join(" · ");
}

function escapeWiqlValue(value: string): string {
  return value.replaceAll("'", "''");
}

function wiqlList(values: readonly string[]): string {
  return values.map((value) => `'${escapeWiqlValue(value)}'`).join(", ");
}

export {
  NOT_STARTED_STATES,
  describeScope,
  escapeWiqlValue,
  extractChildIds,
  jsonOption,
  loadWorkItem,
  normalizeItem,
  printItems,
  printJson,
  queryTasks,
  resolveSprint,
  runListAction,
  sharedListOptions,
  teamOption,
  topOption,
  wiqlList,
};
export type { ListItem };
