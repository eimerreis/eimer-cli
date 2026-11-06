import { defineCommand, option } from "@bunli/core";
import { describeSource } from "@scripts/config";
import { bold, printError, printInfo } from "@scripts/ui";
import { z } from "zod";
import { contextOption, getDefaultAreaPath, getDefaultTeam, initContext } from "../utils";

const showCommand = defineCommand({
  name: "show",
  description: "Show the active context and where it was resolved from",
  options: {
    context: contextOption,
    json: option(z.coerce.boolean().default(false), {
      argumentKind: "flag",
      short: "j",
      description: "Print machine-readable JSON",
    }),
  },
  handler: async ({ flags }) => {
    try {
      const context = await initContext(flags.context, { quiet: flags.json });
      const team = await getDefaultTeam();
      const areaPath = await getDefaultAreaPath();

      if (flags.json) {
        console.log(
          JSON.stringify(
            context
              ? { ...context, team, areaPath }
              : { name: null, source: null, team, areaPath },
            null,
            2,
          ),
        );
        return;
      }

      if (!context) {
        printInfo(
          "No context active - org/project are detected from the git remote.",
          "Create one with `task context use <name>` after defining it under `contexts` in ~/.config/eimer/config.json.",
        );
        return;
      }

      console.log(`${bold("Context")}:   ${context.name}`);
      console.log(`${bold("Org")}:       ${context.orgUrl}`);
      console.log(`${bold("Project")}:   ${context.project}`);
      console.log(`${bold("Team")}:      ${team}`);
      console.log(`${bold("Area path")}: ${areaPath}`);
      console.log(`${bold("Source")}:    ${describeSource(context)}`);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      printError(`Failed to show context: ${message}`);
      process.exit(1);
    }
  },
});

export default showCommand;
