import { defineCommand, option } from "@bunli/core";
import { printError, printInfo, printSuccess, terminalLink, withSpinner } from "@scripts/ui";
import { z } from "zod";
import { buildWorkItemUrl, getStateEmoji, resolveIdArg, runJson, tryGetAzureContext } from "./utils";

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
        throw new Error("Missing task ID. Usage: task start [id]");
      }

      const startTask = () =>
        runJson<UpdatedWorkItem>([
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
      const updated = flags.json
        ? await startTask()
        : await withSpinner(`Starting task #${id}`, startTask, { silentFailure: true, silentSuccess: true });

      if (flags.json) {
        console.log(JSON.stringify(updated, null, 2));
        return;
      }

      const context = await tryGetAzureContext();
      const url = buildWorkItemUrl(updated.id, context);
      const state = updated.fields["System.State"] || "Active";
      const title = updated.fields["System.Title"] || "Untitled";
      const line = `${getStateEmoji(state)} #${updated.id} ${title}`;
      printSuccess(`Started task ${terminalLink(`#${updated.id}`, url)}.`);
      printInfo(line);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      printError(`Failed to start task: ${message}`);
      process.exit(1);
    }
  },
});

export default startCommand;
