import { defineCommand, option } from "@bunli/core";
import { printError, printSuccess, terminalLink, withSpinner } from "@scripts/ui";
import { z } from "zod";
import {
  buildRunUrl,
  formatRunStatus,
  getRepoInfo,
  parseKeyValuePairs,
  resolveStringArg,
  runJson,
  type PipelineDefinition,
  type PipelineRun,
} from "./utils";

const triggerCommand = defineCommand({
  name: "trigger",
  description: "Queue a new pipeline run",
  options: {
    id: option(z.coerce.number().int().positive().optional(), {
      short: "i",
      description: "Pipeline definition ID",
    }),
    name: option(z.string().trim().optional(), {
      short: "m",
      description: "Pipeline definition name",
    }),
    branch: option(z.string().trim().optional(), {
      short: "b",
      description: "Branch name or full ref",
    }),
    parameter: option(z.array(z.string()).default([]), {
      short: "p",
      description: "Pipeline parameter key=value (repeatable)",
    }),
    variable: option(z.array(z.string()).default([]), {
      short: "v",
      description: "Pipeline variable key=value (repeatable)",
    }),
    json: option(z.coerce.boolean().default(false), {
      short: "j",
      description: "Print machine-readable JSON",
    }),
  },
  handler: async ({ flags, positional, prompt }) => {
    try {
      const repoInfo = await withSpinner("Detecting repository", () => getRepoInfo(), {
        silentFailure: true,
        silentSuccess: true,
      });
      const pipelineName = resolveStringArg(flags.name, positional);
      const pipeline = await withSpinner(
        "Resolving pipeline",
        () =>
          resolvePipeline(flags.id, pipelineName, repoInfo.repositoryFilter, repoInfo.repositoryType, prompt),
        { silentFailure: true },
      );

      const command = [
        "az",
        "pipelines",
        "run",
        "--id",
        String(pipeline.id),
        "--detect",
        "true",
        "--output",
        "json",
      ];

      if (flags.branch) {
        command.push("--branch", flags.branch);
      }

      const parameters = parseKeyValuePairs(flags.parameter);
      if (parameters.length > 0) {
        command.push("--parameters", ...parameters);
      }

      const variables = parseKeyValuePairs(flags.variable);
      if (variables.length > 0) {
        command.push("--variables", ...variables);
      }

      const run = await withSpinner("Queueing pipeline run", () => runJson<PipelineRun>(command), {
        silentFailure: true,
      });

      if (flags.json) {
        console.log(JSON.stringify(run, null, 2));
        return;
      }

      const line = `${formatRunStatus(run.status, run.result)} ${terminalLink(
        `#${run.id}`,
        buildRunUrl(run.id),
      )} ${run.definition?.name || pipeline.name || `Pipeline ${pipeline.id}`}`;
      printSuccess(`Queued pipeline run ${line}`);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      printError(`Failed to trigger pipeline: ${message}`);
      process.exit(1);
    }
  },
});

async function resolvePipeline(
  id: number | undefined,
  name: string | undefined,
  repositoryFilter: string,
  repositoryType: "tfsgit" | "github",
  prompt: {
    text(
      message: string,
      options?: {
        fallbackValue?: string;
        placeholder?: string;
        validate?: (value: string) => true | string;
      },
    ): Promise<string>;
  },
): Promise<PipelineDefinition> {
  if (id) {
    const definition = await runJson<PipelineDefinition>([
      "az",
      "pipelines",
      "show",
      "--id",
      String(id),
      "--detect",
      "true",
      "--output",
      "json",
    ]);
    return definition;
  }

  if (name) {
    const definitions = await runJson<PipelineDefinition[]>([
      "az",
      "pipelines",
      "list",
      "--name",
      name,
      "--top",
      "20",
      "--detect",
      "true",
      "--output",
      "json",
    ]);

    const exact = definitions.find((item) => (item.name || "").toLowerCase() === name.toLowerCase());
    if (!exact) {
      throw new Error(`No pipeline found with name '${name}'.`);
    }

    return exact;
  }

  const repoDefinitions = await runJson<PipelineDefinition[]>([
    "az",
    "pipelines",
    "list",
    "--repository",
    repositoryFilter,
    "--repository-type",
    repositoryType,
    "--top",
    "100",
    "--detect",
    "true",
    "--output",
    "json",
  ]);

  if (repoDefinitions.length === 1) {
    return repoDefinitions[0];
  }

  if (repoDefinitions.length === 0) {
    throw new Error(
      `No pipelines found for repo '${repositoryFilter}' (${repositoryType}). Pass --id or --name to select pipeline directly.`,
    );
  }

  const choices = repoDefinitions.slice(0, 10).map((item) => `#${item.id} ${item.name || "Unnamed"}`).join("\n");
  const chosen = (
    await prompt.text(`Select pipeline ID:\n${choices}`, {
      fallbackValue: "",
      placeholder: String(repoDefinitions[0].id),
      validate: (value) => {
        const parsed = Number.parseInt(value.trim(), 10);
        return Number.isFinite(parsed) && parsed > 0 ? true : "Pipeline ID must be a positive integer";
      },
    })
  ).trim();

  const chosenId = Number.parseInt(chosen, 10);
  if (!Number.isFinite(chosenId) || chosenId <= 0) {
    throw new Error("Missing pipeline ID. Pass --id or use interactive selection.");
  }

  const matched = repoDefinitions.find((item) => item.id === chosenId);
  if (matched) {
    return matched;
  }

  const definition = await runJson<PipelineDefinition>([
    "az",
    "pipelines",
    "show",
    "--id",
    String(chosenId),
    "--detect",
    "true",
    "--output",
    "json",
  ]);
  return definition;
}

export default triggerCommand;
