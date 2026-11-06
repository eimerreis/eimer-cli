import { defineCommand, option } from "@bunli/core";
import { loadConfig, updateConfig, withHomePath, writeRcFile } from "@scripts/config";
import { printError, printSuccess, printWarning } from "@scripts/ui";
import { z } from "zod";

const useCommand = defineCommand({
  name: "use",
  description: "Activate a context for the current directory (writes .eimerrc)",
  options: {
    global: option(z.coerce.boolean().default(false), {
      argumentKind: "flag",
      short: "g",
      description: "Set defaultContext in config.json instead of writing .eimerrc",
    }),
  },
  handler: async ({ flags, positional, cwd }) => {
    try {
      const name = positional[0]?.trim();
      if (!name) {
        throw new Error("Missing context name. Usage: task context use <name> [--global]");
      }

      const config = await loadConfig();
      const known = Object.keys(config.contexts ?? {});
      if (!config.contexts?.[name]) {
        const hint = known.length > 0 ? `Known contexts: ${known.join(", ")}.` : "No contexts are defined in config.json.";
        throw new Error(`Unknown context '${name}'. ${hint}`);
      }

      if (flags.global) {
        await updateConfig((current) => ({ ...current, defaultContext: name }));
        printSuccess(`Set default context to '${name}'.`);
      } else {
        const path = await writeRcFile(cwd, name);
        printSuccess(`Using context '${name}' in ${withHomePath(path)}.`);
      }

      if (process.env.EIMER_CONTEXT?.trim()) {
        printWarning(`EIMER_CONTEXT is set to '${process.env.EIMER_CONTEXT.trim()}' and takes precedence.`);
      }
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      printError(`Failed to switch context: ${message}`);
      process.exit(1);
    }
  },
});

export default useCommand;
