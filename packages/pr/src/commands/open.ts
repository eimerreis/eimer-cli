import { defineCommand, option } from "@bunli/core";
import { printError, printInfo, printSuccess, withSpinner } from "@scripts/ui";
import { findAzurePrByBranch, findAzurePrById } from "./comments-azdo";
import { findGitHubPrByBranch, findGitHubPrById, parseGitHubRepo } from "./comments-github";
import { detectPlatform, resolveIdArg, runText } from "./comments-utils";
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

      if (platform === "github") {
        await withSpinner("Opening GitHub pull request", () => openGitHubPr(remoteUrl, branch, prId, flags.repo), {
          silentFailure: true,
          silentSuccess: true,
        });
        return;
      }

      await withSpinner("Opening Azure DevOps pull request", () => openAzurePr(branch, prId, flags.repo), {
        silentFailure: true,
        silentSuccess: true,
      });
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      printError(`Failed to open PR: ${message}`, "Make sure your browser opener is available and your GitHub or Azure CLI session is authenticated.");
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
    printInfo(`No pull request found for branch '${branch}'.`, "Try `pr list --all` or pass --id to open a specific PR.");
    return;
  }

  if (!pr.html_url) {
    printError(`Found PR #${pr.number} but could not determine a web URL.`);
    process.exit(1);
  }

  await runText(["open", pr.html_url]);
  printSuccess(`Opened PR #${pr.number}: ${pr.title}`);
}

async function openAzurePr(branch: string, providedPrId?: number, repoOverride?: string): Promise<void> {
  const pr = providedPrId
    ? await findAzurePrById(providedPrId)
    : await tryFindAzurePrByBranch(branch, repoOverride);

  if (!pr) {
    printInfo(`No pull request found for branch '${branch}'.`, "Try `pr list --all` or pass --id to open a specific PR.");
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

  printSuccess(`Opened PR #${pr.pullRequestId}: ${pr.title}`);
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
