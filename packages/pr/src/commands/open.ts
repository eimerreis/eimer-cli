import { defineCommand, option } from "@bunli/core";
import { findAzurePrByBranch, findAzurePrById } from "./comments-azdo";
import { findGitHubPrByBranch, findGitHubPrById, parseGitHubRepo } from "./comments-github";
import { detectPlatform, runText } from "./comments-utils";
import { z } from "zod";

const openCommand = defineCommand({
  name: "open",
  description: "Open pull request for current branch",
  options: {
    id: option(z.coerce.number().int().positive().optional(), {
      short: "i",
      description: "Pull request ID/number override",
    }),
    branch: option(z.string().trim().optional(), {
      short: "b",
      description: "Branch override for PR lookup",
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

      if (platform === "github") {
        await openGitHubPr(remoteUrl, branch, flags.id, flags.repo);
        return;
      }

      await openAzurePr(branch, flags.id, flags.repo);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      console.error(`Failed to open PR: ${message}`);
      process.exit(1);
    }
  },
});

async function openGitHubPr(
  remoteUrl: string,
  branch: string,
  providedPrId?: number,
  repoOverride?: string,
): Promise<void> {
  const repo = parseGitHubRepo(remoteUrl, repoOverride);

  const pr = providedPrId
    ? await findGitHubPrById(repo, providedPrId)
    : await tryFindGitHubPrByBranch(repo, branch);

  if (!pr) {
    console.log(`No pull request found for branch '${branch}'.`);
    return;
  }

  if (!pr.html_url) {
    console.error(`Found PR #${pr.number} but could not determine a web URL.`);
    process.exit(1);
  }

  await runText(["open", pr.html_url]);
  console.log(`Opened PR #${pr.number}: ${pr.title}`);
}

async function openAzurePr(branch: string, providedPrId?: number, repoOverride?: string): Promise<void> {
  const pr = providedPrId
    ? await findAzurePrById(providedPrId)
    : await tryFindAzurePrByBranch(branch, repoOverride);

  if (!pr) {
    console.log(`No pull request found for branch '${branch}'.`);
    return;
  }

  await runText([
    "az",
    "repos",
    "pr",
    "show",
    "--id",
    String(pr.pullRequestId),
    "--open",
    "--detect",
    "true",
    "--output",
    "none",
  ]);

  console.log(`Opened PR #${pr.pullRequestId}: ${pr.title}`);
}

async function tryFindGitHubPrByBranch(remote: { owner: string; name: string }, branch: string) {
  try {
    return await findGitHubPrByBranch(remote, branch);
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    if (message.includes("No active GitHub PR found")) {
      return null;
    }

    throw error;
  }
}

async function tryFindAzurePrByBranch(branch: string, repoOverride?: string) {
  try {
    return await findAzurePrByBranch(branch, repoOverride);
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    if (message.includes("No active Azure DevOps PR found")) {
      return null;
    }

    throw error;
  }
}

export default openCommand;
