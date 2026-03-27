#!/usr/bin/env tsx

import { command, option, positional, run, string } from "cmd-ts";
import { $ } from "execa";
import { PullRequestListItem } from "./pr-types";
import { TerminalLink } from "../util";
import { buildPullRequestUrl, getAzureProjectBaseUrl, getDefaultReviewer } from "./utils";

const getVoteEmoji = (vote: number) => {
  // no vote
  if (vote === 0) {
    return "👀";
  }

  // approved with suggestions
  if (vote === 5) {
    return "👍";
  }

  // waiting for author
  if (vote === -5) {
    return "⌛";
  }

  // rejected
  if (vote === -10) {
    return "👎";
  }

  return "🤷";
};

const cmd = command({
  name: "prs",
  description: "Lists PRS that are interesting for me",
  version: "0.0.1",
  args: {
    reviewer: option({
      type: string,
      long: "reviewer",
      short: "r",
      defaultValue: () => "",
      description: "Reviewer to filter by",
    }),
  },
  handler: async ({ reviewer: reviewerArg }) => {
    const baseUrl = await getAzureProjectBaseUrl();
    const reviewerFilter = (reviewerArg || (await getDefaultReviewer())).trim().toLowerCase();
    if (!reviewerFilter) {
      throw new Error("Pass --reviewer or set AZURE_DEVOPS_REVIEWER (or git user.email).");
    }

    const pullRequests = JSON.parse(
      (await $`az repos pr list --status active --top 50`).stdout
    ) as PullRequestListItem[];

    const prsWithReviewState = pullRequests.reduce<
      Array<{
        pr: PullRequestListItem;
        reviewer: PullRequestListItem["reviewers"][0];
      }>
    >((acc, pr) => {
      const reviewer = pr.reviewers.find(
        (x) => x.uniqueName.toLowerCase() === reviewerFilter
      );

      if (reviewer) {
        return [...acc, { pr, reviewer }];
      }

      return acc;
    }, []);
    for (const { pr, reviewer } of prsWithReviewState) {
      const logString = `${getVoteEmoji(reviewer.vote)} - ${pr.title} ${
        !pr.createdBy.displayName.startsWith("Project Collection")
          ? ` -- 👤 ${pr.createdBy.displayName}`
          : ""
      }`;
      const url = buildPullRequestUrl(baseUrl, pr.repository.name, pr.pullRequestId);

      console.log(TerminalLink(logString, url));
    }
  },
});

run(cmd, process.argv.slice(2));
