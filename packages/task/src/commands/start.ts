import { defineCommand, option } from "@bunli/core";
import { z } from "zod";
import { buildWorkItemUrl, getStateEmoji, runJson, terminalLink, tryGetAzureContext } from "./utils";

type UpdatedWorkItem = {
  id: number;
  fields: {
    "System.Title"?: string;
    "System.State"?: string;
  };
};

const startCommand = defineCommand({
  name: "start",
  description: "Start a task",
  options: {
    id: option(z.coerce.number().int().positive().optional(), {
      short: "i",
      description: "Task ID",
    }),
    json: option(z.coerce.boolean().default(false), {
      short: "j",
      description: "Print machine-readable JSON",
    }),
  },
  handler: async ({ flags, positional, prompt }) => {
    try {
      let id = flags.id;
      if (!id && positional.length > 0) {
        const parsed = Number.parseInt(positional[0], 10);
        id = Number.isFinite(parsed) ? parsed : undefined;
      }

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
        throw new Error("Missing task ID. Usage: task start --id <number>");
      }

      const updated = await runJson<UpdatedWorkItem>([
        "az",
        "boards",
        "work-item",
        "update",
        "--id",
        String(id),
        "--state",
        "Active",
        "--detect",
        "true",
        "--output",
        "json",
      ]);

      if (flags.json) {
        console.log(JSON.stringify(updated, null, 2));
        return;
      }

      const context = await tryGetAzureContext();
      const url = buildWorkItemUrl(updated.id, context);
      const state = updated.fields["System.State"] || "Active";
      const title = updated.fields["System.Title"] || "Untitled";
      const line = `${getStateEmoji(state)} #${updated.id} ${title}`;
      console.log(terminalLink(line, url));
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      console.error(`Failed to start task: ${message}`);
      process.exit(1);
    }
  },
});

export default startCommand;
