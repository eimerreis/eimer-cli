#!/usr/bin/env tsx

import { $ } from "execa";
import { PipelineRunResult } from "./pipeline-run-types";
import { TerminalLink } from "../util";
import { buildBuildUrl, getAzureProjectBaseUrl, getRequestedForIdentity } from "./utils";

const getEmoji = (status: string, result: string): string => {
  if (status === "inProgress") return "🔵";
  if (status === "completed") {
    if (result === "succeeded") return "🟢";
    if (result === "failed") return "🔴";
    if (result === "canceled") return "⚪";
    return "⚫";
  }
  if (status === "notStarted") return "⏸️";
  return "⚫";
};

const calculateDuration = (startTime?: string, finishTime?: string): string => {
  if (!startTime || startTime === "null") return "Not started";

  try {
    const start = new Date(startTime).getTime();
    const end =
      finishTime && finishTime !== "null"
        ? new Date(finishTime).getTime()
        : Date.now();

    const elapsed = Math.floor((end - start) / 1000);
    const minutes = Math.floor(elapsed / 60);
    const seconds = elapsed % 60;

    const duration = minutes > 0 ? `${minutes}m ${seconds}s` : `${seconds}s`;
    return finishTime && finishTime !== "null"
      ? duration
      : `${duration} (running)`;
  } catch (error) {
    return "Calculating...";
  }
};

const main = async () => {
  const baseUrl = await getAzureProjectBaseUrl();
  const requestedFor = await getRequestedForIdentity();
  let repoUrl = "";
  try {
    repoUrl = (await $`git remote get-url origin`).stdout.trim();
  } catch {
    console.error("❌ Not in a git repository");
    process.exit(1);
  }

  const repoName = (await $`basename ${repoUrl} .git`).stdout.trim();

  const pipelineRunsJson = requestedFor
    ? await $`az pipelines runs list --requested-for ${requestedFor}`
    : await $`az pipelines runs list`;
  const allRuns: PipelineRunResult[] = JSON.parse(pipelineRunsJson.stdout);

  // Filter by repo
  const filteredRuns = allRuns.filter(
    (run) => run.repository.name === repoName
  );

  if (filteredRuns.length === 0) {
    console.log(`❌ No pipeline runs found for repository: ${repoName}`);
    process.exit(0);
  }

  // Sort by queue time and take top 3
  const runs = filteredRuns
    .sort(
      (a, b) =>
        new Date(b.queueTime).getTime() - new Date(a.queueTime).getTime()
    )
    .slice(0, 3);

  // Output formatted results
  for (const run of runs) {
    const emoji = getEmoji(run.status, run.result);
    const message = run.triggerInfo?.["ci.message"]
      ? run.triggerInfo["ci.message"]
      : run.triggerInfo?.["pr.number"]
      ? `Build for PullRequest ${run.triggerInfo["pr.number"]}`
      : "No commit message";

    const duration = calculateDuration(run.startTime, run.finishTime);
    const webUrl = buildBuildUrl(baseUrl, run.id);

    console.log(`${emoji} ${message}`);
    console.log(TerminalLink(`${run.definition.name} • ${duration}`, webUrl));
    console.log("");
  }
};

main();
