import { defineCommand, option } from "@bunli/core";
import { printError, withSpinner } from "@scripts/ui";
import { z } from "zod";
import { loadAzureComments } from "./comments-azdo";
import { loadGitHubComments } from "./comments-github";
import { detectPlatform, printHumanReadable, resolveIdArg, runText } from "./comments-utils";

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
  handler: async ({ flags, positional, prompt }) => {
    try {
      const remoteUrl = (await runText(["git", "remote", "get-url", "origin"])).trim();
      const platform = detectPlatform(remoteUrl);
      const prId = resolveIdArg(flags.id, positional);
      let branch = flags.branch?.trim() || "";

      if (!branch && !prId) {
        branch = (await runText(["git", "branch", "--show-current"])).trim();
      }

      if (!branch && !prId) {
        branch = (await prompt.text("Branch name", {
          placeholder: "feature/my-branch",
          fallbackValue: "",
          validate: (value) => (value.trim().length > 0 ? true : "Branch is required"),
        })).trim();

        if (!branch) {
          printError("Could not determine branch. Pass [id], --branch, or --id.", "Run from a checked out branch or pass --branch explicitly.");
          process.exit(1);
        }
      }

      const result = await withSpinner(
        "Loading review comments",
        () =>
          platform === "github"
            ? loadGitHubComments(remoteUrl, branch, prId, flags.repo)
            : loadAzureComments(branch, prId, flags.repo),
        { silentFailure: true },
      );

      if (flags.json) {
        console.log(JSON.stringify(result, null, 2));
        return;
      }

      printHumanReadable(result);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      printError(`Failed to list PR comments: ${message}`, "Check your PR lookup inputs and make sure `gh` or `az` is authenticated.");
      process.exit(1);
    }
  },
});

export default commentsCommand;
