import { defineCommand, option } from "@bunli/core";
import { loadConfig, resolveContext } from "@scripts/config";
import { colors, printError, printInfo, renderTable, symbols } from "@scripts/ui";
import { z } from "zod";

const listCommand = defineCommand({
  name: "list",
  description: "List contexts defined in config.json",
  options: {
    json: option(z.coerce.boolean().default(false), {
      argumentKind: "flag",
      short: "j",
      description: "Print machine-readable JSON",
    }),
  },
  handler: async ({ flags }) => {
    try {
      const config = await loadConfig();
      const contexts = Object.entries(config.contexts ?? {});
      const active = await resolveContext({ config }).catch(() => null);

      if (flags.json) {
        console.log(
          JSON.stringify(
            contexts.map(([name, definition]) => ({ name, ...definition, active: active?.name === name })),
            null,
            2,
          ),
        );
        return;
      }

      if (contexts.length === 0) {
        printInfo(
          "No contexts defined.",
          'Add them to ~/.config/eimer/config.json, e.g. "contexts": { "tapio": { "org": "tapioone", "project": "tapio" } }',
        );
        return;
      }

      const rows = contexts.map(([name, definition]) => [
        active?.name === name ? colors.success(symbols.ok) : "",
        name,
        definition.org,
        definition.project,
        definition.team ?? "-",
        definition.areaPath ?? "-",
      ]);

      console.log(renderTable(["", "Context", "Org", "Project", "Team", "Area path"], rows, { wordWrap: true }));
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      printError(`Failed to list contexts: ${message}`);
      process.exit(1);
    }
  },
});

export default listCommand;
