import { defineCommand, option } from "@bunli/core";
import { deleteConfig, getConfigPath, loadConfig, saveConfig, withHomePath, type EimerConfig } from "@scripts/config";
import { printError, printSuccess } from "@scripts/ui";
import { z } from "zod";

const configurableKeys = [
  "teams.webhookUrl",
  "task.defaultTeam",
  "task.defaultAreaPath",
  "release.defaultPipeline",
] as const;

type ConfigKey = (typeof configurableKeys)[number];

const configureCommand = defineCommand({
  name: "configure",
  description: "Manage shared CLI defaults",
  options: {
    show: option(z.coerce.boolean().default(false), {
      short: "s",
      description: "Show current config",
    }),
    reset: option(z.coerce.boolean().default(false), {
      short: "r",
      description: "Delete config file",
    }),
    key: option(z.enum(configurableKeys).optional(), {
      short: "k",
      description: "Config key to set",
    }),
    value: option(z.string().optional(), {
      short: "v",
      description: "Config value to set; empty string clears",
    }),
  },
  handler: async ({ flags, prompt }) => {
    try {
      const path = withHomePath(getConfigPath());

      if (flags.reset) {
        await deleteConfig();
        printSuccess(`Deleted config at ${path}.`);
        return;
      }

      if (flags.show) {
        const current = await loadConfig();
        console.log(JSON.stringify(current, null, 2));
        return;
      }

      if (flags.key) {
        const next = setConfigValue(await loadConfig(), flags.key, flags.value?.trim() || "");
        await saveConfig(next);
        printSuccess(`Updated ${flags.key} in ${path}.`);
        return;
      }

      const current = await loadConfig();
      const next = await promptForConfig(current, prompt);
      await saveConfig(next);
      printSuccess(`Saved config to ${path}.`);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      printError(`Failed to configure eimer: ${message}`, "Try `eimer configure --show` to inspect the current config before changing values.");
      process.exit(1);
    }
  },
});

async function promptForConfig(
  current: EimerConfig,
  prompt: {
    text(
      message: string,
      options?: {
        fallbackValue?: string;
        placeholder?: string;
      },
    ): Promise<string>;
  },
): Promise<EimerConfig> {
  const webhookUrl = (await prompt.text("Teams webhook URL", {
    placeholder: "https://...",
    fallbackValue: current.teams?.webhookUrl || "",
  })).trim();

  const defaultTeam = (await prompt.text("Default Azure DevOps team", {
    placeholder: "Default Team",
    fallbackValue: current.task?.defaultTeam || "",
  })).trim();

  const defaultAreaPath = (await prompt.text("Default Azure DevOps area path", {
    placeholder: "Company\\Engineering",
    fallbackValue: current.task?.defaultAreaPath || "",
  })).trim();

  const defaultPipeline = (await prompt.text("Default release pipeline name", {
    placeholder: "example-release-pipeline",
    fallbackValue: current.release?.defaultPipeline || "",
  })).trim();

  return pruneEmpty({
    teams: {
      webhookUrl,
    },
    task: {
      defaultTeam,
      defaultAreaPath,
    },
    release: {
      defaultPipeline,
    },
  });
}

function setConfigValue(current: EimerConfig, key: ConfigKey, value: string): EimerConfig {
  const next = structuredClone(current);

  if (key === "teams.webhookUrl") {
    next.teams = {
      ...(next.teams || {}),
      webhookUrl: value || undefined,
    };
  }

  if (key === "task.defaultTeam") {
    next.task = {
      ...(next.task || {}),
      defaultTeam: value || undefined,
    };
  }

  if (key === "task.defaultAreaPath") {
    next.task = {
      ...(next.task || {}),
      defaultAreaPath: value || undefined,
    };
  }

  if (key === "release.defaultPipeline") {
    next.release = {
      ...(next.release || {}),
      defaultPipeline: value || undefined,
    };
  }

  return pruneEmpty(next);
}

function pruneEmpty(config: EimerConfig): EimerConfig {
  return {
    teams: config.teams?.webhookUrl ? { webhookUrl: config.teams.webhookUrl } : undefined,
    task:
      config.task?.defaultTeam || config.task?.defaultAreaPath
        ? {
            defaultTeam: config.task?.defaultTeam || undefined,
            defaultAreaPath: config.task?.defaultAreaPath || undefined,
          }
        : undefined,
    release: config.release?.defaultPipeline ? { defaultPipeline: config.release.defaultPipeline } : undefined,
  };
}

export default configureCommand;
