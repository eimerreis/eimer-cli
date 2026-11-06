import { defineCommand, option } from "@bunli/core";
import { printInfo } from "@scripts/ui";
import { z } from "zod";
import {
  describeScope,
  escapeWiqlValue,
  printItems,
  printJson,
  queryTasks,
  resolveSprint,
  runListAction,
  sharedListOptions,
  teamOption,
} from "./shared";

const teamCommand = defineCommand({
  name: "team",
  description: "List active tasks in the team's current sprint",
  options: {
    team: teamOption,
    all: option(z.coerce.boolean().default(false), {
      argumentKind: "flag",
      short: "a",
      description: "Include non-active states",
    }),
    ...sharedListOptions,
  },
  handler: async ({ flags }) => {
    await runListAction("Loading sprint tasks", flags, async () => {
      const { team, iterationPath } = await resolveSprint(flags.team);
      const stateFilter = flags.all ? "" : " AND [System.State] = 'Active'";
      const items = (
        await queryTasks(
          `[System.IterationPath] = '${escapeWiqlValue(iterationPath)}'${stateFilter}`,
          "[System.AssignedTo] ASC, [System.ChangedDate] DESC",
        )
      ).slice(0, flags.top);

      if (flags.json) {
        printJson({ mode: "sprint", team, iterationPath, items });
        return;
      }

      if (items.length === 0) {
        printInfo(
          `No ${flags.all ? "tasks" : "active tasks"} found for '${iterationPath}'.`,
          flags.all ? describeScope(team) : "Try --all to include non-active states.",
        );
        return;
      }

      printInfo(`Sprint ${iterationPath}`, describeScope(team));
      await printItems(items);
    });
  },
});

export default teamCommand;
