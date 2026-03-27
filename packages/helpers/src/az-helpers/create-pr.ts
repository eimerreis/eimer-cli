#!/usr/bin/env tsx

import { command, run, positional, string } from "cmd-ts";
import { $ } from "execa";
import { buildPullRequestUrl, getAzureProjectBaseUrl } from "./utils";

const cmd = command({
  name: "create-pr",
  description: "Create a PR for the current branch",
  version: "0.0.1",
  args: {
    title: positional({
      type: string,
      description: "Title of the PR",
    }),
  },
  handler: async ({ title }) => {
    const branch = await $`git branch-name`;
    const repoUrl = await $`git remote get-url origin`;
    const repoName = await $`basename ${repoUrl} .git`;
    const baseUrl = await getAzureProjectBaseUrl();

    const prOutput =
      await $`az repos pr create --repository ${repoName} --source-branch ${branch} --title ${title}`;

    const prJson = JSON.parse(prOutput?.stdout);
    const prId = prJson.pullRequestId;
    const prUrl = buildPullRequestUrl(baseUrl, repoName.stdout.trim(), prId);
    const commitMessage = `Merged PR ${prId}: ${title}`
      .replace('"', "")
      .replace("'", "");

    // update pr and set auto complete
    await $`az repos pr update --id ${prId} --merge-commit-message ${commitMessage} --auto-complete true --squash`;

    if (!prUrl) {
      console.error("PR created but couldn't extract URL");
      console.error(prOutput);
      process.exit(1);
    }

    await $`open ${prUrl}`;
  },
});

run(cmd, process.argv.slice(2));
