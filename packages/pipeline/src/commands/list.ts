import { defineCommand, option } from "@bunli/core";
import { printError, printInfo, renderTable, withSpinner } from "@scripts/ui";
import { z } from "zod";
import { runJson } from "./utils";

type PipelineListItem = {
  id: number;
  name?: string;
  path?: string;
  revision?: number;
  queueStatus?: string;
};

const listCommand = defineCommand({
  name: "list",
  description: "List pipelines, optionally filtered by glob",
  options: {
    filter: option(z.array(z.string()).default([]), {
      short: "f",
      description: "Glob filter on pipeline name (repeatable)",
    }),
    top: option(z.coerce.number().int().positive().default(1000), {
      short: "n",
      description: "Maximum number of pipelines to fetch",
    }),
    json: option(z.coerce.boolean().default(false), {
      short: "j",
      description: "Print machine-readable JSON",
    }),
  },
  handler: async ({ flags, positional }) => {
    try {
      const patterns = [...flags.filter, ...positional].map((item) => item.trim()).filter((item) => item.length > 0);
      const matchers = patterns.map((pattern) => ({
        pattern,
        regex: globPatternToRegex(pattern),
      }));

      const allPipelines = await withSpinner(
        "Loading pipelines",
        () =>
          runJson<PipelineListItem[]>([
            "az",
            "pipelines",
            "list",
            "--top",
            String(flags.top),
            "--detect",
            "true",
            "--output",
            "json",
          ]),
        { silentFailure: true },
      );

      const filtered = allPipelines
        .filter((pipeline) => matchesPipelineName(pipeline.name || "", matchers))
        .sort((left, right) => (left.name || "").localeCompare(right.name || "", undefined, { sensitivity: "base" }));

      if (flags.json) {
        console.log(
          JSON.stringify(
            {
              totalFetched: allPipelines.length,
              count: filtered.length,
              filters: patterns,
              items: filtered,
            },
            null,
            2,
          ),
        );
        return;
      }

      if (filtered.length === 0) {
        if (patterns.length === 0) {
          printInfo("No pipelines found.");
          return;
        }

        printInfo(`No pipelines match: ${patterns.join(", ")}.`, "Try --filter '*name*' or increase --top.");
        return;
      }

      const rows = filtered.map((pipeline) => [
        `#${pipeline.id}`,
        pipeline.name || "Unnamed",
        pipeline.path || "\\",
        pipeline.queueStatus || "-",
        pipeline.revision ?? "-",
      ]);

      console.log(renderTable(["Pipeline", "Name", "Path", "Queue", "Rev"], rows, { wordWrap: true }));
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      printError(`Failed to list pipelines: ${message}`);
      process.exit(1);
    }
  },
});

function matchesPipelineName(name: string, matchers: Array<{ pattern: string; regex: RegExp }>): boolean {
  if (matchers.length === 0) {
    return true;
  }

  return matchers.some((matcher) => matcher.regex.test(name));
}

function globPatternToRegex(pattern: string): RegExp {
  let value = "^";

  for (const char of pattern) {
    if (char === "*") {
      value += ".*";
      continue;
    }

    if (char === "?") {
      value += ".";
      continue;
    }

    value += escapeRegexCharacter(char);
  }

  value += "$";
  return new RegExp(value, "i");
}

function escapeRegexCharacter(char: string): string {
  return /[\\^$.*+?()[\]{}|]/.test(char) ? `\\${char}` : char;
}

export default listCommand;
