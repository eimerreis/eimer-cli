import { defineCommand, option } from "@bunli/core";
import { z } from "zod";
import { findAzurePrByBranch, findAzurePrById } from "./comments-azdo";
import { findGitHubPrByBranch, findGitHubPrById, parseGitHubRepo } from "./comments-github";
import { detectPlatform, resolveIdArg, runText } from "./comments-utils";

type CopyTarget = {
  id: number;
  title: string;
  url: string;
  platform: "github" | "azure-devops";
};

const copyCommand = defineCommand({
  name: "copy",
  description: "Copy PR reference with link to clipboard",
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
    json: option(z.coerce.boolean().default(false), {
      short: "j",
      description: "Print machine-readable JSON",
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
        branch = (
          await prompt.text("Branch name", {
            placeholder: "feature/my-branch",
            fallbackValue: "",
            validate: (value) => (value.trim().length > 0 ? true : "Branch is required"),
          })
        ).trim();

        if (!branch) {
          throw new Error("Could not determine branch. Pass [id], --branch, or --id.");
        }
      }

      const target =
        platform === "github"
          ? await resolveGitHubTarget(remoteUrl, branch, prId, flags.repo)
          : await resolveAzureTarget(remoteUrl, branch, prId, flags.repo);

      if (!target) {
        const lookup = prId ? `ID '${prId}'` : `branch '${branch}'`;
        console.log(`No pull request found for ${lookup}.`);
        return;
      }

      const markdown = `[Pull Request ${target.id}: ${target.title}](${target.url})`;
      await copyToClipboard(markdown);

      if (flags.json) {
        console.log(
          JSON.stringify(
            {
              copied: true,
              text: markdown,
              pr: target,
            },
            null,
            2,
          ),
        );
        return;
      }

      console.log(`Copied to clipboard: ${markdown}`);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      console.error(`Failed to copy PR link: ${message}`);
      process.exit(1);
    }
  },
});

async function resolveGitHubTarget(
  remoteUrl: string,
  branch: string,
  providedPrId?: number,
  repoOverride?: string,
): Promise<CopyTarget | null> {
  const repo = parseGitHubRepo(remoteUrl, repoOverride);
  const pr = providedPrId ? await findGitHubPrById(repo, providedPrId) : await tryFindGitHubPrByBranch(repo, branch);
  if (!pr || !pr.html_url) {
    return null;
  }

  return {
    id: pr.number,
    title: pr.title,
    url: pr.html_url,
    platform: "github",
  };
}

async function resolveAzureTarget(
  remoteUrl: string,
  branch: string,
  providedPrId?: number,
  repoOverride?: string,
): Promise<CopyTarget | null> {
  const pr = providedPrId ? await findAzurePrById(providedPrId) : await tryFindAzurePrByBranch(branch, repoOverride);
  if (!pr) {
    return null;
  }

  const repoName = repoOverride || pr.repository?.name || parseRepoName(remoteUrl);
  const baseUrl = parseAzureProjectBaseUrl(remoteUrl);
  if (!baseUrl) {
    throw new Error("Could not determine Azure DevOps project URL from git remote.");
  }

  return {
    id: pr.pullRequestId,
    title: pr.title,
    url: `${baseUrl}/_git/${repoName}/pullrequest/${pr.pullRequestId}`,
    platform: "azure-devops",
  };
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

function parseRepoName(remoteUrl: string): string {
  const match = remoteUrl.match(/[:/]([^/:]+?)(?:\.git)?$/);
  const repoName = match?.[1];
  if (!repoName) {
    throw new Error(`Could not parse repository name from remote URL: ${remoteUrl}`);
  }

  return repoName;
}

function parseAzureProjectBaseUrl(remoteUrl: string): string {
  const httpsMatch = remoteUrl.match(/^https:\/\/dev\.azure\.com\/([^/]+)\/([^/]+)\/_git\//i);
  if (httpsMatch) {
    return `https://dev.azure.com/${httpsMatch[1]}/${httpsMatch[2]}`;
  }

  const sshMatch = remoteUrl.match(/^(?:ssh:\/\/)?git@ssh\.dev\.azure\.com:v3\/([^/]+)\/([^/]+)\/[^/]+(?:\.git)?$/i);
  if (sshMatch) {
    return `https://dev.azure.com/${sshMatch[1]}/${sshMatch[2]}`;
  }

  const vsSshMatch = remoteUrl.match(/^(?:ssh:\/\/)?[^@]+@([^.]+)\.vs-ssh\.visualstudio\.com:v3\/([^/]+)\/[^/]+(?:\.git)?$/i);
  if (vsSshMatch) {
    return `https://${vsSshMatch[1]}.visualstudio.com/${vsSshMatch[2]}`;
  }

  return "";
}

async function copyToClipboard(text: string): Promise<void> {
  if (process.platform === "darwin") {
    await writeToClipboardCommand(["pbcopy"], text);
    return;
  }

  if (process.platform === "win32") {
    await writeToClipboardCommand(["clip"], text);
    return;
  }

  try {
    await writeToClipboardCommand(["wl-copy"], text);
    return;
  } catch {}

  await writeToClipboardCommand(["xclip", "-selection", "clipboard"], text);
}

async function writeToClipboardCommand(command: string[], text: string): Promise<void> {
  const process = Bun.spawn({
    cmd: command,
    stdin: "pipe",
    stdout: "pipe",
    stderr: "pipe",
  });

  if (!process.stdin) {
    throw new Error(`Could not open stdin for clipboard command '${command.join(" ")}'.`);
  }

  const writeResult = process.stdin.write(text);
  if (writeResult instanceof Promise) {
    await writeResult;
  }
  process.stdin.end();

  const [stderr, exitCode] = await Promise.all([new Response(process.stderr).text(), process.exited]);
  if (exitCode !== 0) {
    throw new Error(`Clipboard command failed (${command.join(" ")}): ${stderr.trim() || `exit code ${exitCode}`}`);
  }
}

export default copyCommand;
