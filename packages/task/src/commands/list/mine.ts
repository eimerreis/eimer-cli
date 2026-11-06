import { defineCommand, option } from "@bunli/core";
import { printInfo } from "@scripts/ui";
import { z } from "zod";
import {
  NOT_STARTED_STATES,
  describeScope,
  escapeWiqlValue,
  printItems,
  printJson,
  queryTasks,
  resolveSprint,
  runListAction,
  sharedListOptions,
  teamOption,
  wiqlList,
} from "./shared";

type ListMineOptions = {
  team?: string;
  all: boolean;
  top: number;
  json: boolean;
  context?: string;
};

const mineOptions = {
  team: teamOption,
  all: option(z.coerce.boolean().default(false), {
    argumentKind: "flag",
    short: "a",
    description: "Include all states (not only not-started tasks)",
  }),
  ...sharedListOptions,
};

async function listMine(flags: ListMineOptions): Promise<void> {
  await runListAction("Loading your tasks", flags, async () => {
    const { team, iterationPath } = await resolveSprint(flags.team);
    const stateFilter = flags.all ? "" : ` AND [System.State] IN (${wiqlList(NOT_STARTED_STATES)})`;
    const items = (
      await queryTasks(
        `[System.AssignedTo] = @Me AND [System.IterationPath] = '${escapeWiqlValue(iterationPath)}'${stateFilter}`,
        "[System.ChangedDate] DESC",
      )
    ).slice(0, flags.top);

    if (flags.json) {
      printJson({ mode: "mine", team, iterationPath, items });
      return;
    }

    if (items.length === 0) {
      printInfo(
        `No ${flags.all ? "" : "not-started "}tasks assigned to you in '${iterationPath}'.`,
        flags.all ? describeScope(team) : "Try --all to include tasks in any state.",
      );
      return;
    }

    printInfo(`My tasks in ${iterationPath}`, describeScope(team));
    await printItems(items);
  });
}

const mineCommand = defineCommand({
  name: "mine",
  description: "List not-started tasks assigned to me in the current sprint",
  options: mineOptions,
  handler: async ({ flags }) => {
    await listMine(flags);
  },
});

export default mineCommand;
export { listMine, mineOptions };
