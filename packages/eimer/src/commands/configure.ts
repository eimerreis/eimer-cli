import { defineCommand, option } from "@bunli/core";
import { deleteConfig, getConfigPath, loadConfig, saveConfig, withHomePath, type EimerConfig } from "@scripts/config";
import { showReleaseConfig } from "@scripts/release/commands";
import { bold, printError, printInfo, printSuccess } from "@scripts/ui";
import { z } from "zod";

const configurableKeys = ["task.defaultTeam", "task.defaultAreaPath"] as const;

type ConfigKey = (typeof configurableKeys)[number];

const releaseHint = "Release settings (pipeline, branch, Teams channels) live in their own file: run `eimer release configure`.";

const configureCommand = defineCommand({
  name: "configure",
  description: "Manage shared CLI defaults (task); see `eimer release configure` for release settings",
  options: {
    show: option(z.coerce.boolean().default(false), {
      argumentKind: "flag",
      short: "s",
      description: "Show current config (eimer + release)",
    }),
    reset: option(z.coerce.boolean().default(false), {
      argumentKind: "flag",
      short: "r",
      description: "Delete eimer config file",
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
        console.log(bold("eimer config"));
        console.log(`Config path: ${path}`);
        console.log(`Default team: ${current.task?.defaultTeam || "(not set)"}`);
        console.log(`Default area path: ${current.task?.defaultAreaPath || "(not set)"}`);
        console.log("");
        console.log(bold("release config"));
        await showReleaseConfig();
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
      printInfo(releaseHint);
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
  const defaultTeam = (await prompt.text("Default Azure DevOps team", {
    placeholder: "Default Team",
    fallbackValue: current.task?.defaultTeam || "",
  })).trim();

  const defaultAreaPath = (await prompt.text("Default Azure DevOps area path", {
    placeholder: "Company\\Engineering",
    fallbackValue: current.task?.defaultAreaPath || "",
  })).trim();

  return pruneEmpty({
    task: {
      defaultTeam,
      defaultAreaPath,
    },
  });
}

function setConfigValue(current: EimerConfig, key: ConfigKey, value: string): EimerConfig {
  const next = structuredClone(current);

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

  return pruneEmpty(next);
}

function pruneEmpty(config: EimerConfig): EimerConfig {
  return {
    task:
      config.task?.defaultTeam || config.task?.defaultAreaPath
        ? {
            defaultTeam: config.task?.defaultTeam || undefined,
            defaultAreaPath: config.task?.defaultAreaPath || undefined,
          }
        : undefined,
  };
}

export default configureCommand;
