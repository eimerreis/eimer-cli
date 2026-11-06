import { defineCommand, option, type Command } from "@bunli/core";
import { z } from "zod";
import { resolveIdArg } from "../utils";
import childrenCommand, { listChildren } from "./children";
import mineCommand, { listMine, mineOptions } from "./mine";
import teamCommand from "./team";

/**
 * `task list` is both a group (`mine`, `team`, `children`) and a runnable command:
 * bare `task list` behaves like `task list mine`. `--parent <id>` and a positional
 * ID are kept as backward-compatible shortcuts for `task list children <id>`.
 */
const listRunnable = defineCommand({
  name: "list",
  description: "List tasks (defaults to `list mine`)",
  options: {
    ...mineOptions,
    parent: option(z.coerce.number().int().positive().optional(), {
      short: "p",
      description: "Deprecated: use `task list children <id>`",
    }),
  },
  handler: async ({ flags, positional }) => {
    const parentId = resolveIdArg(flags.parent, positional);
    if (parentId) {
      await listChildren(parentId, flags);
      return;
    }

    await listMine(flags);
  },
});

// Bunli's types forbid runnable commands with subcommands, but its runtime supports both.
const listCommand = {
  ...listRunnable,
  commands: [mineCommand, teamCommand, childrenCommand],
} as unknown as Command;

export default listCommand;
