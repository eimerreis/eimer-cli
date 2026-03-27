import { defineCommand, option } from "@bunli/core";
import { z } from "zod";
import { buildWorkItemUrl, getStateEmoji, resolveIdArg, runJson, terminalLink, tryGetAzureContext } from "./utils";

type UpdatedWorkItem = {
  id: number;
  fields: {
    "System.Title"?: string;
    "System.State"?: string;
  };
};

const closeCommand = defineCommand({
  name: "close",
  description: "Close a task",
  options: {
    id: option(z.coerce.number().int().positive().optional(), {
      short: "i",
      description: "Task ID",
    }),
    json: option(z.coerce.boolean().default(false), {
      short: "j",
      description: "Print machine-readable JSON",
    }),
    completed: option(z.coerce.number().nonnegative().optional(), {
      short: "c",
      description: "Completed work hours (also sets remaining to 0)",
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
        throw new Error("Missing task ID. Usage: task close [id]");
      }

      const command = [
        "az",
        "boards",
        "work-item",
        "update",
        "--id",
        String(id),
        "--state",
        "Closed",
        "--detect",
        "true",
        "--output",
        "json",
      ];

      if (flags.completed !== undefined) {
        command.push(
          "--fields",
          `Microsoft.VSTS.Scheduling.CompletedWork=${flags.completed}`,
          "Microsoft.VSTS.Scheduling.RemainingWork=0",
        );
      }

      const updated = await runJson<UpdatedWorkItem>(command);

      if (flags.json) {
        console.log(JSON.stringify(updated, null, 2));
        return;
      }

      const context = await tryGetAzureContext();
      const url = buildWorkItemUrl(updated.id, context);
      const state = updated.fields["System.State"] || "Closed";
      const title = updated.fields["System.Title"] || "Untitled";
      const line = `${getStateEmoji(state)} #${updated.id} ${title}`;
      console.log(terminalLink(line, url));
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      console.error(`Failed to close task: ${message}`);
      process.exit(1);
    }
  },
});

export default closeCommand;
