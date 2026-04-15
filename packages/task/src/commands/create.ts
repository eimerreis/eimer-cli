import { defineCommand, option } from "@bunli/core";
import { printError, printInfo, printSuccess, terminalLink, withSpinner } from "@scripts/ui";
import { z } from "zod";
import {
  buildWorkItemUrl,
  extractAssignedTo,
  getDefaultAreaPath,
  getDefaultTeam,
  getStateEmoji,
  loadCurrentIterationByTeam,
  runJson,
  tryGetAzureContext,
} from "./utils";

type CreatedWorkItem = {
  id: number;
  fields: {
    "System.Title"?: string;
    "System.State"?: string;
    "System.AssignedTo"?: { displayName?: string; uniqueName?: string } | string;
  };
};

const createCommand = defineCommand({
  name: "create",
  description: "Create an Azure DevOps task",
  options: {
    parent: option(z.coerce.number().int().positive().optional(), {
      short: "p",
      description: "Parent work item ID (Story/Epic)",
    }),
    team: option(z.string().trim().optional(), {
      short: "t",
      description: "Team name for active iteration lookup",
    }),
    estimate: option(z.coerce.number().nonnegative().optional(), {
      short: "e",
      description: "Original estimate hours",
    }),
    remaining: option(z.coerce.number().nonnegative().optional(), {
      short: "r",
      description: "Remaining work hours",
    }),
    completed: option(z.coerce.number().nonnegative().optional(), {
      short: "c",
      description: "Completed work hours",
    }),
    json: option(z.coerce.boolean().default(false), {
      short: "j",
      description: "Print machine-readable JSON",
    }),
  },
  handler: async ({ positional, flags, prompt }) => {
    try {
      const defaultAreaPath = await getDefaultAreaPath();
      const defaultTeam = await getDefaultTeam();

      let title = positional.join(" ").trim();
      if (!title) {
        title = (
          await prompt.text("Task title", {
            placeholder: "Implement API pagination",
            fallbackValue: "",
            validate: (value) => (value.trim().length > 0 ? true : "Title is required"),
          })
        ).trim();
      }

      if (!title) {
        throw new Error("Missing task title. Usage: task create \"your title\". Pass a title or answer the interactive prompt.");
      }

      const iterationPath = await resolveCurrentIterationPath(flags.team, defaultTeam, prompt);
      const originalEstimate = await resolveOriginalEstimate(flags.estimate, flags.remaining, prompt);

      const command = [
        "az",
        "boards",
        "work-item",
        "create",
        "--type",
        "Task",
        "--title",
        title,
        "--area",
        defaultAreaPath,
        "--iteration",
        iterationPath,
        "--detect",
        "true",
        "--output",
        "json",
      ];

      const fieldPairs: string[] = [];
      fieldPairs.push(`Microsoft.VSTS.Scheduling.OriginalEstimate=${originalEstimate}`);

      if (flags.completed !== undefined) {
        fieldPairs.push(`Microsoft.VSTS.Scheduling.CompletedWork=${flags.completed}`);
      }

      if (flags.remaining !== undefined) {
        fieldPairs.push(`Microsoft.VSTS.Scheduling.RemainingWork=${flags.remaining}`);
      }

      if (fieldPairs.length > 0) {
        command.push("--fields", ...fieldPairs);
      }

      const createTask = () => runJson<CreatedWorkItem>(command);
      const created = flags.json
        ? await createTask()
        : await withSpinner("Creating task", createTask, { silentFailure: true, silentSuccess: true });

      if (flags.parent) {
        const linkParent = () =>
          runJson<unknown>([
            "az",
            "boards",
            "work-item",
            "relation",
            "add",
            "--id",
            String(created.id),
            "--relation-type",
            "parent",
            "--target-id",
            String(flags.parent),
            "--detect",
            "true",
            "--output",
            "json",
          ]);

        if (flags.json) {
          await linkParent();
        } else {
          await withSpinner(`Linking task #${created.id} to parent #${flags.parent}`, linkParent, {
            silentFailure: true,
            silentSuccess: true,
          });
        }
      }

      if (flags.json) {
        console.log(JSON.stringify(created, null, 2));
        return;
      }

      const context = await tryGetAzureContext();
      const url = buildWorkItemUrl(created.id, context);
      const state = created.fields["System.State"] || "Unknown";
      const titleText = created.fields["System.Title"] || title;
      const assignedTo = extractAssignedTo(created.fields["System.AssignedTo"]);
      const linkText = `${getStateEmoji(state)} #${created.id} ${titleText} | ${assignedTo}`;
      printSuccess(`Created task ${terminalLink(`#${created.id}`, url)}.`);
      printInfo(linkText, flags.parent ? `Linked to parent #${flags.parent}.` : undefined);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      printError(`Failed to create task: ${message}`, "Check your Azure DevOps defaults with `eimer configure --show` or pass explicit task options.");
      process.exit(1);
    }
  },
});

export default createCommand;

async function resolveOriginalEstimate(
  estimate: number | undefined,
  remaining: number | undefined,
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
): Promise<number> {
  if (estimate !== undefined) {
    return estimate;
  }

  if (remaining !== undefined) {
    return remaining;
  }

  const raw = (
    await prompt.text("Original estimate (hours)", {
      placeholder: "4",
      fallbackValue: "",
      validate: (value) => {
        const parsed = Number.parseFloat(value.trim());
        return Number.isFinite(parsed) && parsed >= 0 ? true : "Estimate must be a non-negative number";
      },
    })
  ).trim();

  const parsed = Number.parseFloat(raw);
  if (!Number.isFinite(parsed) || parsed < 0) {
    throw new Error("Missing original estimate. Pass --estimate or --remaining.");
  }

  return parsed;
}

async function resolveCurrentIterationPath(
  teamOverride: string | undefined,
  defaultTeam: string,
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
): Promise<string> {
  const team = teamOverride?.trim() || defaultTeam;
  const fromTeam = await loadCurrentIterationByTeam(team);
  if (fromTeam) {
    return fromTeam;
  }

  if (teamOverride?.trim()) {
    throw new Error(`No active iteration found for team '${team}'. Confirm the team name or re-run with a different --team value.`);
  }

  const promptedTeam = (
    await prompt.text("Azure DevOps team name", {
      placeholder: defaultTeam,
      fallbackValue: "",
      validate: (value) => (value.trim().length > 0 ? true : "Team name is required"),
    })
  ).trim();

  if (!promptedTeam) {
    throw new Error(`No active iteration found for default team '${defaultTeam}'. Pass --team to override or update your defaults with 'eimer configure'.`);
  }

  const promptedIteration = await loadCurrentIterationByTeam(promptedTeam);
  if (!promptedIteration) {
    throw new Error(`No active iteration found for team '${promptedTeam}'. Confirm the team exists and has a current sprint configured.`);
  }

  return promptedIteration;
}
