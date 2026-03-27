import { defineCommand, option } from "@bunli/core";
import { z } from "zod";
import {
  buildWorkItemUrl,
  extractAssignedTo,
  extractParentId,
  formatRelativeTime,
  getStateEmoji,
  resolveIdArg,
  runJson,
  terminalLink,
  tryGetAzureContext,
  type WorkItem,
} from "./utils";

type ShowNode = {
  id: number;
  title: string;
  description?: string;
  state: string;
  type: string;
  assignedTo: string;
  changedDate?: string;
};

const showCommand = defineCommand({
  name: "show",
  description: "Show a task and parent hierarchy",
  options: {
    id: option(z.coerce.number().int().positive().optional(), {
      short: "i",
      description: "Task ID",
    }),
    parents: option(z.coerce.boolean().default(true), {
      short: "p",
      description: "Include parent hierarchy (Task -> Story -> Epic)",
    }),
    "allow-non-task": option(z.coerce.boolean().default(false), {
      short: "n",
      description: "Allow non-task root items (Bug/Story/etc)",
    }),
    json: option(z.coerce.boolean().default(false), {
      short: "j",
      description: "Print machine-readable JSON",
    }),
  },
  handler: async ({ flags, positional, prompt }) => {
    try {
      let id = resolveIdArg(flags.id, positional);

      if (!id) {
        const raw = (
          await prompt.text("Task ID", {
            placeholder: "12345",
            fallbackValue: "",
            validate: (value) => {
              const parsed = Number.parseInt(value.trim(), 10);
              return Number.isFinite(parsed) && parsed > 0 ? true : "Task ID must be a positive integer";
            },
          })
        ).trim();

        id = Number.parseInt(raw, 10);
      }

      if (!id || !Number.isFinite(id) || id <= 0) {
        throw new Error("Missing task ID. Usage: task show [id]");
      }

      const chain = flags.parents ? await loadParentChain(id) : [await loadWorkItem(id)];
      const root = chain[0];

      if (!flags["allow-non-task"] && root && root.type.toLowerCase() !== "task") {
        throw new Error(
          `Work item #${root.id} is '${root.type}', not 'Task'. Pass --allow-non-task to show it anyway.`,
        );
      }

      if (flags.json) {
        console.log(JSON.stringify(chain, null, 2));
        return;
      }

      printHierarchy(chain);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      console.error(`Failed to show task: ${message}`);
      process.exit(1);
    }
  },
});

async function loadWorkItem(id: number): Promise<ShowNode & { parentId: number | null }> {
  const workItem = await runJson<WorkItem>([
    "az",
    "boards",
    "work-item",
    "show",
    "--id",
    String(id),
    "--expand",
    "relations",
    "--detect",
    "true",
    "--output",
    "json",
  ]);

  const parentId = extractParentId(workItem.relations);
  return {
    id,
    title: workItem.fields["System.Title"] || "Untitled",
    description: workItem.fields["Custom.Description"] || workItem.fields["System.Description"],
    state: workItem.fields["System.State"] || "Unknown",
    type: workItem.fields["System.WorkItemType"] || "Work Item",
    assignedTo: extractAssignedTo(workItem.fields["System.AssignedTo"]),
    changedDate: workItem.fields["System.ChangedDate"],
    parentId,
  };
}

async function loadParentChain(id: number): Promise<ShowNode[]> {
  const chain: ShowNode[] = [];
  const visited = new Set<number>();
  let currentId: number | null = id;

  while (currentId && !visited.has(currentId)) {
    visited.add(currentId);
    const item = await loadWorkItem(currentId);
    chain.push({
      id: item.id,
      title: item.title,
      description: item.description,
      state: item.state,
      type: item.type,
      assignedTo: item.assignedTo,
      changedDate: item.changedDate,
    });
    currentId = item.parentId;
  }

  return chain;
}

async function printHierarchy(chain: ShowNode[]): Promise<void> {
  if (chain.length === 0) {
    console.log("No task data found.");
    return;
  }

  const context = await tryGetAzureContext();
  for (let index = 0; index < chain.length; index += 1) {
    const item = chain[index];
    const indent = "  ".repeat(index);
    const connector = index === 0 ? "" : "`- ";
    const typePrefix = item.type.toUpperCase();
    const changed = formatRelativeTime(item.changedDate);
    const changedSuffix = changed ? ` | ${changed}` : "";
    const line = `${indent}${connector}${typePrefix} ${getStateEmoji(item.state)} #${item.id} ${item.title} | ${item.assignedTo}${changedSuffix}`;
    console.log(terminalLink(line, buildWorkItemUrl(item.id, context)));
  }
}

export default showCommand;
