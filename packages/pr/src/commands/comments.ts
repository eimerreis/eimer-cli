import { defineCommand, option } from "@bunli/core";
import { z } from "zod";
import { loadAzureComments } from "./comments-azdo";
import { loadGitHubComments } from "./comments-github";
import { detectPlatform, printHumanReadable, runText } from "./comments-utils";

const commentsCommand = defineCommand({
  name: "comments",
  description: "List pull request review comments",
  options: {
    id: option(z.coerce.number().int().positive().optional(), {
      short: "i",
      description: "Pull request ID/number override",
    }),
    json: option(z.coerce.boolean().default(false), {
      short: "j",
      description: "Print machine-readable JSON",
    }),
    branch: option(z.string().trim().optional(), {
      short: "b",
      description: "Branch override for PR auto-detection",
    }),
    repo: option(z.string().trim().optional(), {
      short: "r",
      description: "Repo override (GitHub: owner/repo, AzDO: repo name)",
    }),
  },
  handler: async ({ flags, prompt }) => {
    try {
      const remoteUrl = (await runText(["git", "remote", "get-url", "origin"])).trim();
      const platform = detectPlatform(remoteUrl);
      let branch = flags.branch?.trim() || "";

      if (!branch && !flags.id) {
        branch = (await runText(["git", "branch", "--show-current"])).trim();
      }

      if (!branch && !flags.id) {
        branch = (await prompt.text("Branch name", {
          placeholder: "feature/my-branch",
          fallbackValue: "",
          validate: (value) => (value.trim().length > 0 ? true : "Branch is required"),
        })).trim();

        if (!branch) {
          console.error("Could not determine branch. Pass --branch or --id.");
          process.exit(1);
        }
      }

      const result =
        platform === "github"
          ? await loadGitHubComments(remoteUrl, branch, flags.id, flags.repo)
          : await loadAzureComments(branch, flags.id, flags.repo);

      if (flags.json) {
        console.log(JSON.stringify(result, null, 2));
        return;
      }

      printHumanReadable(result);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      console.error(`Failed to list PR comments: ${message}`);
      process.exit(1);
    }
  },
});

export default commentsCommand;
