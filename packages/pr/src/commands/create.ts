import { defineCommand } from "@bunli/core";
import { printError, printInfo, printSuccess, withSpinner } from "@scripts/ui";
import { runJson, runText } from "./comments-utils";

type CreatedPullRequest = {
  pullRequestId: number;
  title: string;
  _links?: {
    web?: {
      href?: string;
    };
  };
};

const createCommand = defineCommand({
  name: "create",
  description: "Create a pull request (with auto-complete on)",
  handler: async ({ positional, prompt }) => {
    let title = positional.join(" ").trim();

    try {
      if (!title) {
        title = (await prompt.text("PR title", {
          placeholder: "feat: ...",
          fallbackValue: "",
          validate: (value) => (value.trim().length > 0 ? true : "Title is required"),
        })).trim();

        if (!title) {
          printError("Missing PR title. Usage: pr create \"your title\"");
          process.exit(1);
        }
      }

      let branch = (await runText(["git", "branch", "--show-current"])).trim();
      if (!branch) {
        branch = (await prompt.text("Branch name", {
          placeholder: "feature/my-branch",
          fallbackValue: "",
          validate: (value) => (value.trim().length > 0 ? true : "Branch is required"),
        })).trim();

        if (!branch) {
          printError("Could not determine current git branch.");
          process.exit(1);
        }
      }

      const repoUrl = (await runText(["git", "remote", "get-url", "origin"])).trim();
      const repoName = parseRepoName(repoUrl);

      const branchPushed = await withSpinner("Ensuring remote branch exists", () => ensureRemoteBranch(branch), {
        silentFailure: true,
        silentSuccess: true,
      });
      if (branchPushed) {
        printInfo(`Pushed branch '${branch}' to origin.`);
      }

      const pr = await withSpinner("Creating pull request", () => runJson<CreatedPullRequest>([
        "az",
        "repos",
        "pr",
        "create",
        "--repository",
        repoName,
        "--source-branch",
        branch,
        "--title",
        title,
        "--detect",
        "true",
        "--output",
        "json",
      ]), { silentFailure: true, silentSuccess: true });

      const prId = pr.pullRequestId;
      const commitMessage = `Merged PR ${prId}: ${title}`
        .replace('"', "")
        .replace("'", "");

      await withSpinner("Configuring auto-complete", () => runText([
        "az",
        "repos",
        "pr",
        "update",
        "--id",
        String(prId),
        "--merge-commit-message",
        commitMessage,
        "--auto-complete",
        "true",
        "--squash",
        "--detect",
        "true",
        "--output",
        "json",
      ]), { silentFailure: true, silentSuccess: true });

      const webUrl = pr._links?.web?.href;
      if (webUrl) {
        await runText(["open", webUrl]);
      } else {
        await runText([
          "az",
          "repos",
          "pr",
          "show",
          "--id",
          String(prId),
          "--open",
          "--detect",
          "true",
          "--output",
          "none",
        ]);
      }

      printSuccess(`Created PR #${prId} for ${repoName} on ${branch}`);
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      printError(`Failed to create PR: ${message}`);
      process.exit(1);
    }
  },
});

function parseRepoName(remoteUrl: string): string {
  const match = remoteUrl.match(/[:/]([^/:]+?)(?:\.git)?$/);
  const repoName = match?.[1];
  if (!repoName) {
    throw new Error(`Could not parse repository name from remote URL: ${remoteUrl}`);
  }

  return repoName;
}

async function ensureRemoteBranch(branch: string): Promise<boolean> {
  const remoteBranch = (await runText(["git", "ls-remote", "--heads", "origin", branch])).trim();
  if (remoteBranch) {
    return false;
  }

  await runText(["git", "push", "-u", "origin", branch]);
  return true;
}

export default createCommand;
