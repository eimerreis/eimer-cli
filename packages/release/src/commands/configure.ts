import { defineCommand, option } from "@bunli/core";
import { printError, printInfo, printSuccess, withSpinner } from "@scripts/ui";
import { z } from "zod";
import { getConfigPath, loadConfig, resolveChannels, saveConfig, withHomePath, type ReleaseConfig } from "../config";
import { detectProdStageName, listPipelineStages, type PipelineStageRecord } from "./prod-stage";
import { getAzureContext, loadPipelineRuns, resolvePipelineByName } from "./utils";

type ConfigurePrompt = {
  text(
    message: string,
    options?: {
      fallbackValue?: string;
      placeholder?: string;
      validate?: (value: string) => true | string;
    },
  ): Promise<string>;
  select(
    message: string,
    options: {
      options: Array<{
        value: string;
        label: string;
        hint?: string;
      }>;
      default?: string;
    },
  ): Promise<string>;
};

const configureCommand = defineCommand({
  name: "configure",
  description: "Configure release defaults and Teams channels",
  options: {
    show: option(z.coerce.boolean().default(false), {
      short: "s",
      description: "Show current release config",
    }),
  },
  handler: async ({ flags, prompt }) => {
    try {
      const configPath = withHomePath(getConfigPath());
      const showRequested = flags.show || hasCliFlag("--show", "-s");

      if (showRequested) {
        const current = await withSpinner("Loading release config", () => loadConfig(), {
          silentFailure: true,
          silentSuccess: true,
        });
        printConfigSummary(current, configPath);
        return;
      }

      const current = await withSpinner("Loading release config", () => loadConfig(), {
        silentFailure: true,
        silentSuccess: true,
      });
      const next = await promptForConfig(current, prompt as unknown as ConfigurePrompt);
      await withSpinner("Saving release config", () => saveConfig(next), {
        silentFailure: true,
        silentSuccess: true,
      });
      printSuccess(`Saved release config to ${configPath}.`);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      printError(`Failed to configure release CLI: ${message}`, "Run `release configure --show` to inspect the current config.");
      process.exit(1);
    }
  },
});

async function promptForConfig(current: ReleaseConfig, prompt: ConfigurePrompt): Promise<ReleaseConfig> {
  const defaultPipeline = (
    await prompt.text("Default release pipeline", {
      placeholder: "example-release-pipeline",
      fallbackValue: current.release?.defaultPipeline || "",
    })
  ).trim();

  let prodStageName = (current.release?.prodStageName || "").trim();
  if (defaultPipeline) {
    const stageOptions = await resolveStageOptions(defaultPipeline);
    prodStageName = await promptForProdStageName(stageOptions, prodStageName, prompt);
  } else {
    prodStageName = (
      await prompt.text("Production stage name (optional)", {
        placeholder: "deploy_production",
        fallbackValue: prodStageName,
      })
    ).trim();
  }

  const channels = await promptForChannels(current, prompt);

  const namedChannels: Record<string, string> = {};
  for (const [name, url] of Object.entries(channels)) {
    if (name === "default") {
      continue;
    }

    namedChannels[name] = url;
  }

  const primaryWebhook = channels.default || undefined;
  return pruneEmptyConfig({
    release: {
      defaultPipeline: defaultPipeline || undefined,
      prodStageName: prodStageName || undefined,
    },
    teams:
      primaryWebhook || Object.keys(namedChannels).length > 0
        ? {
            webhookUrl: primaryWebhook,
            channels: Object.keys(namedChannels).length > 0 ? namedChannels : undefined,
          }
        : undefined,
    areas: current.areas,
  });
}

async function resolveStageOptions(pipelineName: string): Promise<string[]> {
  const context = await withSpinner("Loading Azure DevOps context", () => getAzureContext(), {
    silentFailure: true,
    silentSuccess: true,
  });
  const pipeline = await withSpinner("Resolving pipeline", () => resolvePipelineByName(pipelineName), {
    silentFailure: true,
    silentSuccess: true,
  });

  const runs = await withSpinner(
    "Loading recent pipeline runs",
    () => loadPipelineRuns({ pipelineId: pipeline.id, top: 30, status: "completed" }),
    {
      silentFailure: true,
      silentSuccess: true,
    },
  );

  for (const run of runs) {
    const stages = await listPipelineStages(context.baseUrl, run.id);
    if (stages.length === 0) {
      continue;
    }

    return collectStageOptionNames(stages);
  }

  return [];
}

function collectStageOptionNames(stages: PipelineStageRecord[]): string[] {
  const inferred = detectProdStageName(stages);
  const names = stages
    .map((stage) => stage.identifier || stage.name)
    .map((value) => value.trim())
    .filter((value) => value.length > 0);

  const unique = Array.from(new Set(names));
  const sorted = unique.sort((left, right) => left.localeCompare(right));

  if (inferred) {
    return [inferred, ...sorted.filter((value) => value !== inferred)];
  }

  return sorted;
}

async function promptForProdStageName(
  stageOptions: string[],
  currentProdStageName: string,
  prompt: ConfigurePrompt,
): Promise<string> {
  if (stageOptions.length === 0) {
    return (
      await prompt.text("Production stage name (optional)", {
        placeholder: "deploy_production",
        fallbackValue: currentProdStageName,
      })
    ).trim();
  }

  printInfo(`Detected pipeline stages: ${stageOptions.join(", ")}`);
  const options: Array<{ value: string; label: string; hint?: string }> = stageOptions.map((name) => ({
    value: name,
    label: name,
  }));
  options.push({
    value: "__custom__",
    label: "Custom stage name",
    hint: "Type an explicit stage identifier or name",
  });
  options.push({
    value: "__none__",
    label: "No prod stage override",
    hint: "Use auto-detection only",
  });

  const defaultChoice = stageOptions.includes(currentProdStageName)
    ? currentProdStageName
    : stageOptions[0] || "__custom__";
  const selected = await prompt.select("Production stage", {
    options,
    default: defaultChoice,
  });

  if (selected === "__none__") {
    return "";
  }

  if (selected === "__custom__") {
    return (
      await prompt.text("Production stage name", {
        placeholder: "deploy_production",
        fallbackValue: currentProdStageName,
      })
    ).trim();
  }

  return selected;
}

async function promptForChannels(current: ReleaseConfig, prompt: ConfigurePrompt): Promise<Record<string, string>> {
  const existingChannels = resolveChannels(current);
  let channels = { ...existingChannels };

  if (Object.keys(existingChannels).length > 0) {
    const action = await prompt.select("Existing Teams channels found", {
      options: [
        {
          value: "keep",
          label: "Keep and edit",
          hint: "Add or overwrite channels",
        },
        {
          value: "reset",
          label: "Start fresh",
          hint: "Discard existing channels in this run",
        },
      ],
      default: "keep",
    });

    if (action === "reset") {
      channels = {};
    }
  }

  while (true) {
    const channelName = (
      await prompt.text("Teams channel name (leave empty to finish)", {
        placeholder: "frontend-releases",
        fallbackValue: "",
      })
    ).trim();

    if (!channelName) {
      return channels;
    }

    const webhookUrl = (
      await prompt.text(`Webhook URL for '${channelName}'`, {
        placeholder: "https://...",
        fallbackValue: channels[channelName] || "",
        validate: (value) => (value.trim().length > 0 ? true : "Webhook URL is required"),
      })
    ).trim();

    channels[channelName] = webhookUrl;
  }
}

function printConfigSummary(config: ReleaseConfig, configPath: string): void {
  const channels = resolveChannels(config);
  const channelNames = Object.keys(channels).sort((left, right) => left.localeCompare(right));

  console.log(`Config path: ${configPath}`);
  console.log(`Default pipeline: ${config.release?.defaultPipeline || "(not set)"}`);
  console.log(`Prod stage override: ${config.release?.prodStageName || "(auto-detect)"}`);
  if (channelNames.length === 0) {
    console.log("Teams channels: (none)");
    return;
  }

  console.log("Teams channels:");
  for (const channelName of channelNames) {
    const webhookUrl = channels[channelName] || "";
    console.log(`- ${channelName}: ${maskWebhookUrl(webhookUrl)}`);
  }
}

function pruneEmptyConfig(config: ReleaseConfig): ReleaseConfig {
  const channels = config.teams?.channels || {};
  const hasNamedChannels = Object.keys(channels).length > 0;
  return {
    teams:
      config.teams?.webhookUrl || hasNamedChannels
        ? {
            webhookUrl: config.teams?.webhookUrl || undefined,
            channels: hasNamedChannels ? channels : undefined,
          }
        : undefined,
    release:
      config.release?.defaultPipeline || config.release?.prodStageName
        ? {
            defaultPipeline: config.release?.defaultPipeline || undefined,
            prodStageName: config.release?.prodStageName || undefined,
          }
        : undefined,
    areas: config.areas,
  };
}

function maskWebhookUrl(value: string): string {
  const trimmed = value.trim();
  if (trimmed.length <= 14) {
    return "***";
  }

  return `${trimmed.slice(0, 10)}...${trimmed.slice(-4)}`;
}

function hasCliFlag(longFlag: string, shortFlag: string): boolean {
  const argv = Bun.argv.slice(2);
  return argv.includes(longFlag) || argv.includes(shortFlag);
}

export default configureCommand;
