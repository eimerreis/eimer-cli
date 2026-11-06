import { defineCommand, option } from "@bunli/core";
import { printError, printInfo, terminalLink } from "@scripts/ui";
import { z } from "zod";
import { buildWorkItemUrl, getStateEmoji, resolveIdArg, tryGetAzureContext } from "../utils";
import {
  extractChildIds,
  loadWorkItem,
  normalizeItem,
  printItems,
  printJson,
  runListAction,
  sharedListOptions,
} from "./shared";

type ListChildrenOptions = {
  all: boolean;
  top: number;
  json: boolean;
  context?: string;
};

async function listChildren(parentId: number, flags: ListChildrenOptions): Promise<void> {
  await runListAction("Loading child tasks", flags, async () => {
    const parent = await loadWorkItem(parentId, true);
    const childIds = extractChildIds(parent.relations);

    const childItems = await Promise.all(childIds.map((id) => loadWorkItem(id, false)));
    const items = childItems
      .map(normalizeItem)
      .filter((item) => item.type.toLowerCase() === "task")
      .filter((item) => (flags.all ? true : item.state.toLowerCase() === "active"))
      .slice(0, flags.top);

    const parentNode = normalizeItem(parent);

    if (flags.json) {
      printJson({ mode: "parent", parent: parentNode, items });
      return;
    }

    const context = await tryGetAzureContext();
    const parentLine = `PARENT ${parentNode.type.toUpperCase()} ${getStateEmoji(parentNode.state)} #${parentNode.id} ${parentNode.title} | ${parentNode.assignedTo}`;
    console.log(terminalLink(parentLine, buildWorkItemUrl(parentNode.id, context)));

    if (items.length === 0) {
      printInfo(
        `No ${flags.all ? "task" : "active task"} children found.`,
        flags.all ? undefined : "Try --all to include non-active child tasks.",
      );
      return;
    }

    await printItems(items);
  });
}

const childrenCommand = defineCommand({
  name: "children",
  description: "List child tasks under a parent work item",
  options: {
    all: option(z.coerce.boolean().default(false), {
      argumentKind: "flag",
      short: "a",
      description: "Include non-active states",
    }),
    ...sharedListOptions,
  },
  handler: async ({ flags, positional }) => {
    const parentId = resolveIdArg(undefined, positional);
    if (!parentId) {
      printError("Missing parent work item ID. Usage: task list children <id>");
      process.exit(1);
    }

    await listChildren(parentId, flags);
  },
});

export default childrenCommand;
export { listChildren };
