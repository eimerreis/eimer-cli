import { defineCommand } from "@bunli/core";
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
          console.error("Missing PR title. Usage: pr create \"your title\"");
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
          console.error("Could not determine current git branch.");
          process.exit(1);
        }
      }

      const repoUrl = (await runText(["git", "remote", "get-url", "origin"])).trim();
      const repoName = parseRepoName(repoUrl);

      const branchPushed = await ensureRemoteBranch(branch);
      if (branchPushed) {
        console.log(`Pushed branch '${branch}' to origin.`);
      }

      const pr = await runJson<CreatedPullRequest>([
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
      ]);

      const prId = pr.pullRequestId;
      const commitMessage = `Merged PR ${prId}: ${title}`
        .replace('"', "")
        .replace("'", "");

      await runText([
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
      ]);

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

      console.log(`Created PR #${prId} for ${repoName} on ${branch}`);
    } catch (err) {
      console.error("Failed to create PR:", err);
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
